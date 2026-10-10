/*
 * @Descripttion:
 * @version:
 * @Author: Wynters
 * @Date: 2024-05-28 20:10:12
 * @LastEditTime: 2024-05-28 20:55:45
 * @FilePath: \RustPanel\src\models\file.rs
 */

use std::{error::Error, fmt, fs};

use encoding_rs::{GBK, UTF_8};
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::io::Read;
use std::time::UNIX_EPOCH;

use crate::common::fun::{sm4_decrypt_file, sm4_encrypt_file};
use crate::service::global::CONF;
use crate::service::path_bounds::{self, BoundPath};

#[derive(Deserialize)]
pub struct SaveData {
    pub path: String,
    pub content: String,
}
// 自定义错误类型
#[derive(Debug)]
pub struct FileError {
    pub message: String,
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for FileError {}

fn bound_err(err: path_bounds::PathBoundError) -> Box<dyn Error> {
    Box::new(FileError {
        message: err.message().to_string(),
    })
}

/// Persist decrypted content via atomic write under an already-bound path.
pub fn save_bounded(bound: BoundPath, content: String) -> Result<bool, Box<dyn Error>> {
    let plain = sm4_decrypt_file(content);
    path_bounds::atomic_write(&bound, plain.as_bytes()).map_err(bound_err)?;
    Ok(true)
}

#[allow(dead_code)]
pub fn save(data: SaveData) -> Result<bool, Box<dyn Error>> {
    let bound = path_bounds::resolve_for_write(&data.path).map_err(bound_err)?;
    save_bounded(bound, data.content)
}

pub fn content_bounded(bound: BoundPath) -> Result<(String, String), Box<dyn Error>> {
    let path = bound.absolute.clone();
    let mut file = path_bounds::open_bound_file(&bound).map_err(bound_err)?;
    let metadata = file.metadata().map_err(|_| {
        Box::new(FileError {
            message: String::from(t!("file.content.info_error")),
        }) as Box<dyn Error>
    })?;

    let mut extension = String::new();

    if let Some(ext) = path.extension().and_then(OsStr::to_str) {
        extension = ext.to_string();
        if extension == "exe" || extension == "dll" || extension == "so" || extension == "zip" {
            return Err(Box::new(FileError {
                message: String::from(t!("file.content.file_type_error")),
            }));
        }
    }

    if metadata.len() as f64 / 1048576.0 > CONF.app.max_file_size {
        return Err(Box::new(FileError {
            message: String::from(t!("file.content.file_max_size_error",size => CONF.app.max_file_size)),
        }));
    }

    let mut content_vec = Vec::new();
    if file.read_to_end(&mut content_vec).is_err() {
        return Err(Box::new(FileError {
            message: String::from(t!("file.content.read_error")),
        }));
    }
    let content = match String::from_utf8(content_vec.clone()) {
        Ok(s) => s,
        Err(_) => convert_gbk_to_utf8(content_vec),
    };

    Ok((sm4_encrypt_file(content.as_str()), extension))
}

pub fn list_bounded(
    bound: BoundPath,
) -> Result<FilesAndDirsInfo, path_bounds::PathBoundError> {
    let entries = path_bounds::open_bound_dir(&bound)?;
    let mut files_info = Vec::new();
    let mut dirs_info = Vec::new();
    let p = bound.absolute.to_string_lossy().into_owned();

    for entry in entries.flatten() {
        let name = match entry.file_name().into_string() {
            Ok(n) => n,
            Err(_) => continue,
        };
        // Use symlink_metadata so directory listings do not follow entry symlinks.
        let metadata = match fs::symlink_metadata(entry.path()) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let modified_time = Some(match metadata.modified() {
            Ok(t) => match t.duration_since(UNIX_EPOCH) {
                Ok(d) => d.as_secs() as u64,
                Err(_) => 0,
            },
            Err(_) => 0,
        });
        if metadata.file_type().is_symlink() {
            // Surface symlink names as files without following them.
            files_info.push(FileInfo {
                name,
                permissions: None,
                size: None,
                modified_time,
            });
        } else if metadata.is_file() {
            files_info.push(FileInfo {
                name,
                permissions: None,
                size: Some(metadata.len()),
                modified_time,
            });
        } else if metadata.is_dir() {
            dirs_info.push(DirInfo {
                name,
                permissions: None,
                modified_time,
            });
        }
    }

    Ok(FilesAndDirsInfo {
        path: p,
        files: files_info,
        dirs: dirs_info,
    })
}

#[allow(dead_code)]
pub fn content(path: String) -> Result<(String, String), Box<dyn Error>> {
    let bound = path_bounds::resolve_existing(&path).map_err(bound_err)?;
    content_bounded(bound)
}

fn convert_gbk_to_utf8(content: Vec<u8>) -> String {
    let (decoded, _, _) = GBK.decode(&content);
    let (encoded, _, _) = UTF_8.encode(&decoded);
    String::from_utf8_lossy(encoded.as_ref()).to_string()
}

#[derive(Serialize)]
pub struct FileInfo {
    pub name: String,
    pub permissions: Option<u32>,
    pub size: Option<u64>,
    pub modified_time: Option<u64>,
}
#[derive(Serialize)]
pub struct DirInfo {
    pub name: String,
    pub permissions: Option<u32>,
    pub modified_time: Option<u64>,
}
#[derive(Serialize)]
pub struct FilesAndDirsInfo {
    pub path: String,
    pub files: Vec<FileInfo>,
    pub dirs: Vec<DirInfo>,
}
