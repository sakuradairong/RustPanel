//! Managed-root path resolution for the file API (RP-06 / C.5).
//!
//! Roots come from `RP_FILE_ROOTS` (colon-separated absolute paths). Setting
//! `RP_FILE_EXPERT_MODE` to `1`/`true`/`yes` adds `/` as an allowed root.
//! When neither is set, defaults are `/www` and `/var/www` (not `/`).
//!
//! On Linux, resolution prefers `openat2(RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS)`
//! and falls back to dirfd + `O_NOFOLLOW` segment walks. Writes use a temp file
//! in the target directory plus `rename`.
use std::{
    env, fs,
    io::{self, Write},
    path::{Component, Path, PathBuf},
};

use rand::{distributions::Alphanumeric, Rng};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathBoundError {
    /// Path leaves managed roots, uses `..`, follows a disallowed symlink, etc.
    Escape(String),
    Io(String),
}

impl PathBoundError {
    pub fn is_escape(&self) -> bool {
        matches!(self, Self::Escape(_))
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Escape(m) | Self::Io(m) => m.as_str(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BoundPath {
    pub absolute: PathBuf,
    pub root: PathBuf,
}

/// App-scoped managed roots (injected via `web::Data` so tests stay race-free).
#[derive(Debug, Clone)]
pub struct FileRootConfig {
    pub roots: Vec<PathBuf>,
}

impl FileRootConfig {
    pub fn from_env() -> Self {
        Self {
            roots: managed_roots(),
        }
    }

    pub fn from_roots(roots: Vec<PathBuf>) -> Self {
        Self { roots }
    }
}

/// Roots currently configured for this process.
pub fn managed_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(raw) = env::var("RP_FILE_ROOTS") {
        for part in raw.split(':') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let path = PathBuf::from(part);
            if path.is_absolute() {
                roots.push(normalize_lexical(&path));
            }
        }
    }
    if expert_mode() {
        let root = PathBuf::from("/");
        if !roots.iter().any(|r| r == &root) {
            roots.push(root);
        }
    }
    if roots.is_empty() {
        roots.push(PathBuf::from("/www"));
        roots.push(PathBuf::from("/var/www"));
    }
    roots
}

fn expert_mode() -> bool {
    matches!(
        env::var("RP_FILE_EXPERT_MODE")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Resolve `path` for read/list: final target must exist beneath a managed root.
pub fn resolve_existing(path: &str) -> Result<BoundPath, PathBoundError> {
    resolve_existing_under(path, &managed_roots())
}

pub fn resolve_existing_under(path: &str, roots: &[PathBuf]) -> Result<BoundPath, PathBoundError> {
    let candidate = parse_request_path(path)?;
    let (root, relative) = match_root(&candidate, roots)?;
    // Always reject symlink components before any open that could follow them.
    reject_symlink_components(&root, &relative)?;
    #[cfg(target_os = "linux")]
    {
        match linux::resolve_beneath(&root, &relative, linux::ResolveMode::Existing) {
            Ok(bound) => return Ok(bound),
            Err(err) if err.is_escape() => return Err(err),
            Err(_) => {}
        }
    }
    resolve_via_canonicalize(&root, &relative, true)
}

/// Resolve `path` for write: parent directory must exist beneath a managed root;
/// the final file may be missing (created atomically later).
pub fn resolve_for_write(path: &str) -> Result<BoundPath, PathBoundError> {
    resolve_for_write_under(path, &managed_roots())
}

pub fn resolve_for_write_under(path: &str, roots: &[PathBuf]) -> Result<BoundPath, PathBoundError> {
    let candidate = parse_request_path(path)?;
    let (root, relative) = match_root(&candidate, roots)?;
    let file_name = relative
        .file_name()
        .ok_or_else(|| PathBoundError::Escape("Write path must name a file".into()))?;
    if file_name == "." || file_name == ".." {
        return Err(PathBoundError::Escape("Invalid write target name".into()));
    }
    let parent_rel = relative
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(""));
    reject_symlink_components(&root, &parent_rel)?;
    #[cfg(target_os = "linux")]
    {
        match linux::resolve_beneath(&root, &parent_rel, linux::ResolveMode::Directory) {
            Ok(parent) => {
                let absolute = parent.absolute.join(file_name);
                if absolute.exists() {
                    let meta = fs::symlink_metadata(&absolute)
                        .map_err(|e| PathBoundError::Io(e.to_string()))?;
                    if meta.file_type().is_symlink() {
                        return Err(PathBoundError::Escape(
                            "Symlinks are not allowed in managed file paths".into(),
                        ));
                    }
                }
                return Ok(BoundPath {
                    absolute,
                    root: parent.root,
                });
            }
            Err(err) if err.is_escape() => return Err(err),
            Err(_) => {}
        }
    }
    let parent_bound = if parent_rel.as_os_str().is_empty() {
        // Writing directly under the managed root.
        ensure_root_dir(&root)?;
        BoundPath {
            absolute: root.clone(),
            root: root.clone(),
        }
    } else {
        resolve_via_canonicalize(&root, &parent_rel, true)?
    };
    if !parent_bound.absolute.is_dir() {
        return Err(PathBoundError::Escape(
            "Write parent is not a directory under a managed root".into(),
        ));
    }
    Ok(BoundPath {
        absolute: parent_bound.absolute.join(file_name),
        root: parent_bound.root,
    })
}

/// Atomically replace/create `bound.absolute` with `bytes` (temp file + rename).
pub fn atomic_write(bound: &BoundPath, bytes: &[u8]) -> Result<(), PathBoundError> {
    let parent = bound.absolute.parent().ok_or_else(|| {
        PathBoundError::Escape("Refusing to write without a parent directory".into())
    })?;
    let name = bound
        .absolute
        .file_name()
        .ok_or_else(|| PathBoundError::Escape("Invalid write target name".into()))?;
    // Parent must still be a real directory (not swapped for a symlink).
    let parent_meta =
        fs::symlink_metadata(parent).map_err(|e| PathBoundError::Io(e.to_string()))?;
    if parent_meta.file_type().is_symlink() || !parent_meta.is_dir() {
        return Err(PathBoundError::Escape(
            "Write parent must be a non-symlink directory under a managed root".into(),
        ));
    }
    let mut tmp_name = name.to_os_string();
    tmp_name.push(".rp-tmp-");
    let suffix: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(12)
        .map(char::from)
        .collect();
    tmp_name.push(suffix);
    let tmp_path = parent.join(&tmp_name);

    let write_result = (|| -> Result<(), PathBoundError> {
        #[cfg(unix)]
        let mut file = {
            use std::os::unix::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&tmp_path)
                .map_err(|e| map_open_err("temp create failed", e))?
        };
        #[cfg(not(unix))]
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
            .map_err(|e| PathBoundError::Io(format!("temp create failed: {e}")))?;
        file.write_all(bytes)
            .map_err(|e| PathBoundError::Io(format!("temp write failed: {e}")))?;
        file.sync_all()
            .map_err(|e| PathBoundError::Io(format!("temp sync failed: {e}")))?;
        fs::rename(&tmp_path, &bound.absolute)
            .map_err(|e| PathBoundError::Io(format!("atomic rename failed: {e}")))?;
        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }
    write_result
}

/// Open a previously bound file without following a leaf symlink (TOCTOU harden).
pub fn open_bound_file(bound: &BoundPath) -> Result<fs::File, PathBoundError> {
    let meta = fs::symlink_metadata(&bound.absolute).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            PathBoundError::Escape("Path not found under managed roots".into())
        } else {
            PathBoundError::Io(e.to_string())
        }
    })?;
    if meta.file_type().is_symlink() {
        return Err(PathBoundError::Escape(
            "Symlinks are not allowed in managed file paths".into(),
        ));
    }
    if !meta.is_file() {
        return Err(PathBoundError::Escape("Path is not a regular file".into()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        return fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&bound.absolute)
            .map_err(|e| map_open_err("open file failed", e));
    }
    #[cfg(not(unix))]
    {
        fs::File::open(&bound.absolute).map_err(|e| PathBoundError::Io(e.to_string()))
    }
}

/// Open a previously bound directory without following a directory symlink.
pub fn open_bound_dir(bound: &BoundPath) -> Result<fs::ReadDir, PathBoundError> {
    let meta = fs::symlink_metadata(&bound.absolute).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            PathBoundError::Escape("Path not found under managed roots".into())
        } else {
            PathBoundError::Io(e.to_string())
        }
    })?;
    if meta.file_type().is_symlink() {
        return Err(PathBoundError::Escape(
            "Symlinks are not allowed in managed file paths".into(),
        ));
    }
    if !meta.is_dir() {
        return Err(PathBoundError::Escape("Path is not a directory".into()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Confirm open with O_NOFOLLOW|O_DIRECTORY before read_dir.
        let _dir = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
            .open(&bound.absolute)
            .map_err(|e| map_open_err("open directory failed", e))?;
        drop(_dir);
    }
    fs::read_dir(&bound.absolute).map_err(|e| PathBoundError::Io(e.to_string()))
}

fn map_open_err(prefix: &str, err: io::Error) -> PathBoundError {
    #[cfg(unix)]
    {
        if err.raw_os_error() == Some(libc::ELOOP) {
            return PathBoundError::Escape(
                "Symlinks are not allowed in managed file paths".into(),
            );
        }
    }
    PathBoundError::Io(format!("{prefix}: {err}"))
}

fn parse_request_path(path: &str) -> Result<PathBuf, PathBoundError> {
    if path.is_empty() || path.contains('\0') {
        return Err(PathBoundError::Escape("Empty or NUL path rejected".into()));
    }
    let raw = PathBuf::from(path);
    for component in raw.components() {
        match component {
            Component::ParentDir => {
                return Err(PathBoundError::Escape(
                    "Path must not contain '..' components".into(),
                ));
            }
            Component::RootDir | Component::CurDir | Component::Normal(_) | Component::Prefix(_) => {
            }
        }
    }
    if !raw.is_absolute() {
        return Err(PathBoundError::Escape(
            "File paths must be absolute under a managed root".into(),
        ));
    }
    Ok(normalize_lexical(&raw))
}

fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir => out.push("/"),
            Component::CurDir => {}
            Component::Normal(s) => out.push(s),
            Component::ParentDir => {
                // Caller already rejects `..`; keep defensive pop for Prefix-less Unix paths.
                out.pop();
            }
            Component::Prefix(p) => out.push(p.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from("/")
    } else {
        out
    }
}

fn match_root<'a>(
    candidate: &Path,
    roots: &'a [PathBuf],
) -> Result<(PathBuf, PathBuf), PathBoundError> {
    let mut best: Option<(usize, &PathBuf)> = None;
    for root in roots {
        if candidate == root.as_path() || candidate.starts_with(root) {
            let len = root.as_os_str().len();
            if best.map(|(l, _)| len > l).unwrap_or(true) {
                best = Some((len, root));
            }
        }
    }
    let Some((_, root)) = best else {
        return Err(PathBoundError::Escape(
            "Path is outside managed file roots".into(),
        ));
    };
    let relative = candidate
        .strip_prefix(root)
        .map_err(|_| PathBoundError::Escape("Path is outside managed file roots".into()))?
        .to_path_buf();
    Ok((root.clone(), relative))
}

fn ensure_root_dir(root: &Path) -> Result<(), PathBoundError> {
    let meta = fs::symlink_metadata(root).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            PathBoundError::Escape("Managed root does not exist".into())
        } else {
            PathBoundError::Io(e.to_string())
        }
    })?;
    if meta.file_type().is_symlink() {
        return Err(PathBoundError::Escape(
            "Managed root must not be a symlink".into(),
        ));
    }
    if !meta.is_dir() {
        return Err(PathBoundError::Escape("Managed root is not a directory".into()));
    }
    Ok(())
}

fn resolve_via_canonicalize(
    root: &Path,
    relative: &Path,
    must_exist: bool,
) -> Result<BoundPath, PathBoundError> {
    ensure_root_dir(root)?;
    let joined = if relative.as_os_str().is_empty() {
        root.to_path_buf()
    } else {
        root.join(relative)
    };
    // Reject any symlink in the relative chain (portable soft-link escape guard).
    reject_symlink_components(root, relative)?;
    if !must_exist && !joined.exists() {
        // Parent already validated by caller for writes.
        return Ok(BoundPath {
            absolute: normalize_lexical(&joined),
            root: root.to_path_buf(),
        });
    }
    let canon = fs::canonicalize(&joined).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            PathBoundError::Escape("Path not found under managed roots".into())
        } else {
            PathBoundError::Io(e.to_string())
        }
    })?;
    let root_canon = fs::canonicalize(root).map_err(|e| PathBoundError::Io(e.to_string()))?;
    if canon != root_canon && !canon.starts_with(&root_canon) {
        return Err(PathBoundError::Escape(
            "Resolved path escapes managed root".into(),
        ));
    }
    Ok(BoundPath {
        absolute: canon,
        root: root.to_path_buf(),
    })
}

fn reject_symlink_components(root: &Path, relative: &Path) -> Result<(), PathBoundError> {
    let mut cursor = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            continue;
        };
        cursor.push(name);
        let meta = match fs::symlink_metadata(&cursor) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(PathBoundError::Io(e.to_string())),
        };
        if meta.file_type().is_symlink() {
            return Err(PathBoundError::Escape(
                "Symlinks are not allowed in managed file paths".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{BoundPath, PathBoundError};
    use libc::{c_char, c_int, c_long, mode_t, ENOSYS};
    use std::{
        ffi::{CString, OsStr},
        fs,
        os::fd::{FromRawFd, IntoRawFd, OwnedFd},
        os::unix::ffi::OsStrExt,
        path::{Component, Path},
    };

    pub enum ResolveMode {
        Existing,
        Directory,
    }

    #[repr(C)]
    struct OpenHow {
        flags: u64,
        mode: u64,
        resolve: u64,
    }

    const RESOLVE_BENEATH: u64 = 0x08;
    const RESOLVE_NO_SYMLINKS: u64 = 0x04;
    // SYS_openat2 is 437 on x86_64 and aarch64.
    const SYS_OPENAT2: c_long = 437;

    pub fn resolve_beneath(
        root: &Path,
        relative: &Path,
        mode: ResolveMode,
    ) -> Result<BoundPath, PathBoundError> {
        let root_fd = open_root(root)?;
        if relative.as_os_str().is_empty() {
            return Ok(BoundPath {
                absolute: root.to_path_buf(),
                root: root.to_path_buf(),
            });
        }
        let rel_cstr = path_to_cstring(relative)?;
        let flags = match mode {
            ResolveMode::Existing => (libc::O_RDONLY | libc::O_NOFOLLOW) as u64,
            ResolveMode::Directory => {
                (libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW) as u64
            }
        };
        let root_raw = root_fd.into_raw_fd();
        let openat2_result = openat2(root_raw, &rel_cstr, flags, 0);
        unsafe { libc::close(root_raw) };
        match openat2_result {
            Ok(fd) => {
                drop(fd);
                // Belt-and-suspenders: never return a path whose leaf is a symlink.
                let absolute = root.join(relative);
                let meta = fs::symlink_metadata(&absolute)
                    .map_err(|e| PathBoundError::Io(e.to_string()))?;
                if meta.file_type().is_symlink() {
                    return Err(PathBoundError::Escape(
                        "Symlinks are not allowed in managed file paths".into(),
                    ));
                }
                Ok(BoundPath {
                    absolute,
                    root: root.to_path_buf(),
                })
            }
            Err(code)
                if code == libc::ENOENT
                    || code == libc::ENOTDIR
                    || code == libc::ELOOP
                    || code == libc::EXDEV =>
            {
                Err(PathBoundError::Escape(
                    "Path escapes managed root or uses disallowed symlink".into(),
                ))
            }
            Err(_) => walk_nofollow(root, relative, mode),
        }
    }

    fn open_root(root: &Path) -> Result<OwnedFd, PathBoundError> {
        let c = path_to_cstring(root)?;
        let fd =
            unsafe { libc::open(c.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC) };
        if fd < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::NotFound {
                return Err(PathBoundError::Escape("Managed root does not exist".into()));
            }
            return Err(PathBoundError::Io(err.to_string()));
        }
        let meta = fs::symlink_metadata(root).map_err(|e| PathBoundError::Io(e.to_string()))?;
        if meta.file_type().is_symlink() {
            unsafe { libc::close(fd) };
            return Err(PathBoundError::Escape(
                "Managed root must not be a symlink".into(),
            ));
        }
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    fn openat2(dirfd: c_int, path: &CString, flags: u64, mode: mode_t) -> Result<OwnedFd, c_int> {
        let how = OpenHow {
            flags,
            mode: mode as u64,
            resolve: RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS,
        };
        let rc = unsafe {
            libc::syscall(
                SYS_OPENAT2,
                dirfd,
                path.as_ptr() as *const c_char,
                &how as *const OpenHow,
                std::mem::size_of::<OpenHow>(),
            )
        };
        if rc < 0 {
            let err = std::io::Error::last_os_error();
            Err(err.raw_os_error().unwrap_or(ENOSYS))
        } else {
            Ok(unsafe { OwnedFd::from_raw_fd(rc as c_int) })
        }
    }

    fn walk_nofollow(
        root: &Path,
        relative: &Path,
        mode: ResolveMode,
    ) -> Result<BoundPath, PathBoundError> {
        let root_owned = open_root(root)?;
        let mut dirfd = root_owned.into_raw_fd();
        let components: Vec<&OsStr> = relative
            .components()
            .filter_map(|c| match c {
                Component::Normal(s) => Some(s),
                Component::CurDir => None,
                Component::ParentDir => None,
                _ => None,
            })
            .collect();
        if components.is_empty() {
            unsafe { libc::close(dirfd) };
            return Ok(BoundPath {
                absolute: root.to_path_buf(),
                root: root.to_path_buf(),
            });
        }
        for (idx, name) in components.iter().enumerate() {
            let is_last = idx + 1 == components.len();
            let cname = os_to_cstring(name)?;
            let flags = if is_last {
                match mode {
                    ResolveMode::Directory => {
                        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC
                    }
                    // Do not use O_PATH here: O_PATH|O_NOFOLLOW can open the symlink inode.
                    ResolveMode::Existing => libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                }
            } else {
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC
            };
            let next = unsafe { libc::openat(dirfd, cname.as_ptr(), flags) };
            unsafe { libc::close(dirfd) };
            if next < 0 {
                let err = std::io::Error::last_os_error();
                let code = err.raw_os_error().unwrap_or(0);
                if code == libc::ELOOP || code == libc::EMLINK {
                    return Err(PathBoundError::Escape(
                        "Symlinks are not allowed in managed file paths".into(),
                    ));
                }
                if err.kind() == std::io::ErrorKind::NotFound {
                    return Err(PathBoundError::Escape(
                        "Path not found under managed roots".into(),
                    ));
                }
                return Err(PathBoundError::Io(err.to_string()));
            }
            dirfd = next;
        }
        unsafe { libc::close(dirfd) };
        Ok(BoundPath {
            absolute: root.join(relative),
            root: root.to_path_buf(),
        })
    }

    fn path_to_cstring(path: &Path) -> Result<CString, PathBoundError> {
        CString::new(path.as_os_str().as_bytes())
            .map_err(|_| PathBoundError::Escape("Path contains interior NUL".into()))
    }

    fn os_to_cstring(name: &OsStr) -> Result<CString, PathBoundError> {
        CString::new(name.as_bytes())
            .map_err(|_| PathBoundError::Escape("Path contains interior NUL".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use tempfile::tempdir;

    fn roots_of(dir: &Path) -> Vec<PathBuf> {
        vec![dir.to_path_buf()]
    }

    #[test]
    fn rejects_parent_dir_and_outside_root() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        assert!(matches!(
            resolve_existing_under(&format!("{}/../etc/passwd", root.display()), &roots_of(root)),
            Err(PathBoundError::Escape(_))
        ));
        assert!(matches!(
            resolve_existing_under("/etc/passwd", &roots_of(root)),
            Err(PathBoundError::Escape(_))
        ));
    }

    #[test]
    fn accepts_file_inside_root_and_rejects_symlink_escape() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let inside = root.join("ok.txt");
        fs::write(&inside, b"hello").unwrap();
        let bound = resolve_existing_under(inside.to_str().unwrap(), &roots_of(root)).unwrap();
        assert_eq!(bound.absolute, inside);

        let link = root.join("escape");
        symlink("/etc/passwd", &link).unwrap();
        assert!(matches!(
            resolve_existing_under(link.to_str().unwrap(), &roots_of(root)),
            Err(PathBoundError::Escape(_))
        ));
    }

    #[test]
    fn atomic_write_creates_and_replaces() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let target = root.join("note.txt");
        let bound = resolve_for_write_under(target.to_str().unwrap(), &roots_of(root)).unwrap();
        atomic_write(&bound, b"one").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"one");
        atomic_write(&bound, b"two").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"two");
        // No leftover temp files.
        let leftovers: Vec<_> = fs::read_dir(root)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .contains(".rp-tmp-")
            })
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn default_roots_exclude_system_root_without_expert_mode() {
        // managed_roots() reads process env; only assert shape of helper defaults via explicit list.
        let defaults = [PathBuf::from("/www"), PathBuf::from("/var/www")];
        assert!(match_root(Path::new("/etc/passwd"), &defaults).is_err());
        assert!(match_root(Path::new("/www/site"), &defaults).is_ok());
    }

    #[test]
    fn open_bound_file_rejects_symlink_leaf_after_resolve_race() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let target = root.join("doc.txt");
        fs::write(&target, b"secret").unwrap();
        let bound = resolve_existing_under(target.to_str().unwrap(), &roots_of(root)).unwrap();
        // Simulate TOCTOU: replace the file with a symlink after resolve.
        fs::remove_file(&target).unwrap();
        symlink("/etc/passwd", &target).unwrap();
        assert!(matches!(
            open_bound_file(&bound),
            Err(PathBoundError::Escape(_))
        ));
    }
}
