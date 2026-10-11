<!--
 * @Descripttion: 
 * @version: 
 * @Author: Wynters
 * @Date: 2024-05-07 21:19:37
 * @LastEditTime: 2024-05-21 17:01:23
 * @FilePath: \RustPanel\README.md
-->
# RustPanel

_language: English, [中文](README.cn-zh.md)

## 🤔 What is this?
![image](https://github.com/WyntersN/RustPanel/assets/27792650/08d1eae1-8161-4e13-a89c-b98bc147fd13)

RustPanel is a Rust server management panel aimed at lightweight, secure, recoverable operations on a single Linux host. Monitoring, file, website and Docker management code and authentication, authorization and session protections exist. An independent privileged runner, complete database management, and backup/restore workflows still need implementation and validation.

> Work is currently focused on the CP0 foundation. Production readiness and supported environments require the evidence described in the development documents.

## Development roadmap

See the [development plan v0.3](docs/development-plan.md) for scope, dependencies and acceptance criteria, and [Rust advantages research](docs/rust-panel-advantages.md) for the rationale. These are delivery goals, not completed features.

| Stage | Delivery goal |
| --- | --- |
| CP0 Foundation | Non-root panel, independent restricted runner, durable operation recovery, secret integration, signed releases and resource baseline |
| CP1 Application recovery | HTTP application + PostgreSQL, Nginx HTTPS, backup, isolated restore and upgrade failure handling on Ubuntu 24.04 x86_64 |
| CP2 Practical replacement | MySQL/Redis/PHP, validated app templates, S3/scheduling/alerts, scoped API tokens, 1Panel inventory and adoption |
| CP3 Demand-driven expansion | Multi-node, plugins and AI integrations after assessing trust and maintenance costs |

Keep Actix/Tokio/SQLite and native JavaScript. Debian/aarch64 require separate validation; Windows/macOS and Kubernetes are outside the initial scope.

## Security keys and development status

The panel now requires independent instance keys in `RP_SECRET_KEY` (at least 32 bytes) and `RP_SESSION_KEY` (at least 64 bytes). Missing, short, identical, or formerly published default keys stop startup. The YAML `app.secret_key` is not the JWT key source.

See [key setup and upgrade notes](docs/security-bootstrap.md), the [development plan](docs/development-plan.md), [implementation progress](docs/development-progress.md), and the research note [Rust advantages for server panels](docs/rust-panel-advantages.md). The first security batch is implemented; the remaining production prerequisites are still in progress.
