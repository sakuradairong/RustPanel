# AGENTS.md

## Cursor Cloud specific instructions

RustPanel is a Rust (actix-web) server management panel. It builds two binaries:
`panel` (the web server, `src/bin/panel.rs`) and `rp` (a small CLI helper,
`src/bin/rp.rs`). The frontend is static files in `public/` served by the panel;
data is stored in a bundled SQLite database (`data.db3`).

### Environment already provided by the VM snapshot
The startup update script runs `rustup default stable` and `cargo fetch`. The
following are already baked into the VM snapshot (do NOT reinstall them):
- Rust stable toolchain (>= 1.85 is required because transitive deps use edition
  2024; the base image's pinned 1.83 is too old, hence `rustup default stable`).
- System package `libssl-dev` (needed to build `openssl-sys`, pulled in via
  `reqwest`). SQLite itself is compiled from source by `rusqlite`'s `bundled`
  feature, so no system SQLite is needed.

### Config files (required, gitignored)
The app reads `./config/conf.yaml` and `./config/log4rs.yaml` at runtime from the
repo-root working directory. These are gitignored and are created from the
committed `*.example` files (`config/conf.yaml.example`, `config/log4rs.yaml.example`).
If they are ever missing, recreate them:
`cp config/conf.yaml.example config/conf.yaml` and
`cp config/log4rs.yaml.example config/log4rs.yaml`.
`conf.yaml` defines the listen host/port (default `0.0.0.0:8080`) and the
`security_dir` used in the login URL.

### Running
- Build (dev): `cargo build` (release packaging is `./build.sh`, which is heavier:
  musl target + UPX, only for producing release artifacts).
- Run the panel: `cargo run --bin panel` (must be run from the repo root — the app
  uses relative paths like `./config`, `./public`, `./data.db3`). Hot-reload dev
  loop is `cargo watch -x 'run --bin panel'` (see `dev-panel.sh`); `cargo-watch` is
  not installed by default.
- The panel exits immediately if its port is already in use.

### Logging in (first-run credentials)
On first startup, `service::db::install()` seeds a random admin user into
`data.db3` and prints its credentials to stdout as `username:<...>` and
`password:<...>` lines. A `panel.lock` file marks install as done — to reseed a
fresh user, stop the panel and delete both `data.db3*` and `panel.lock`, then
start again. Log in at `http://<host>:8080/#/login?v=<security_dir>` (the
`security_dir` value comes from `config/conf.yaml`, default `dfhg156d1`). Login
credentials are SM4-encrypted client-side, so log in through the browser UI rather
than by hand-crafting curl requests.

### Lint / test
- Lint: `cargo clippy` (currently emits warnings only; there is no deny config).
- Tests: `cargo test` (the repo currently has no `#[test]` functions, so this
  compiles and reports 0 tests). CI (`.github/workflows/rust.yml`) runs
  `cargo build` + `cargo test` on the `dev` branch.
