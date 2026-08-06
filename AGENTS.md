# Agent notes

## Cursor Cloud specific instructions

### Services
- **Panel web UI**: `cargo run --bin panel` (listens on `config/conf.yaml` → default `0.0.0.0:8080`). Static assets are served from `public/` (no separate frontend build).
- **CLI**: `cargo run --bin rp -- …`.
- Login path includes `security_dir` from config, e.g. `http://host:8080/#/login?v=dfhg156d1`.

### Docker interactive terminal
- Route: `GET /api/v1/docker/containers/{id}/terminal` (WebSocket upgrade).
- Browsers cannot set `Authorization` on WebSocket; pass JWT as `?token=` (optional `?shell=/bin/bash`, default `/bin/sh`).
- UI entry: Containers → card/drawer **Terminal**, or Exec modal → **Open Terminal**.
- Soft reload may cache `pages.js` / `ui.js`; hard-refresh after pulling frontend changes.

### Environment caveats
- systemd is typically unavailable in Cloud Agent VMs → Services tab is limited.
- Firewall/iptables often unavailable without root.
- Docker (if needed) uses fuse-overlayfs + iptables-legacy in this environment.

### Lint / test / run
- Build: `cargo build --bin panel`
- Tests: `cargo test` (unit/integration as present)
- Format/clippy: `cargo fmt`, `cargo clippy` when touching Rust
- No Node toolchain required for the panel UI (vanilla JS in `public/`).
