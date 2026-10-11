<!--
 * @Descripttion: 
 * @version: 
 * @Author: Wynters
 * @Date: 2024-05-21 17:01:41
 * @LastEditTime: 2024-05-29 21:31:16
 * @FilePath: \RustPanel\README.cn-zh.md
-->
# RustPanel

_语言: 中文, [English](README.md)

## 🤔 这是什么?
![image](https://github.com/WyntersN/RustPanel/assets/27792650/5080e1e5-24f5-4fdc-a85e-be45061626b5)

RustPanel 是基于 Rust 的服务器管理面板，目标是提供轻量、安全、可恢复的单机 Linux 运维。已有监控、文件、网站和 Docker 管理代码，以及认证、权限和会话保护；独立特权执行器、完整数据库管理与备份恢复仍需开发和验收。

> 当前正在完善 CP0 可信底座，尚未完成生产准入；支持范围与验收状态以开发文档为准。

## 开发路线

完整范围、依赖和验收条件见[开发方案 v0.3](docs/development-plan.md)，技术依据见[Rust 优势研究](docs/rust-panel-advantages.md)。下表是交付目标，不是已完成列表。

| 阶段 | 交付目标 |
| --- | --- |
| CP0 可信底座 | 非 root 面板、独立受限 runner、任务结果恢复、秘密接入、发布验签与资源基线 |
| CP1 应用恢复闭环 | Ubuntu 24.04 x86_64 上完成 HTTP 应用 + PostgreSQL、Nginx HTTPS、备份、隔离恢复与升级故障处理 |
| CP2 实用替代 | MySQL/Redis/PHP、验证过的应用模板、S3/调度/告警、作用域 API token、1Panel 盘点与接管 |
| CP3 按需求扩展 | 多节点、插件、AI 集成；先评估信任模型和维护成本 |

保留 Actix/Tokio/SQLite 与原生 JS。Debian/aarch64 经独立验收后扩展；Windows/macOS 和 K8s 暂不进入主线。

## 安全密钥与开发进度

面板现在要求配置独立的实例密钥：`RP_SECRET_KEY`（至少 32 字节）和 `RP_SESSION_KEY`（至少 64 字节）。缺失、过短、相同或沿用曾公开的默认值都会拒绝启动。YAML 中的 `app.secret_key` 不是 JWT 密钥来源。

参见[密钥配置及升级说明](docs/security-bootstrap.md)、[开发方案](docs/development-plan.md)、[实施进度](docs/development-progress.md)和[《Rust 作为服务器 Web 面板的优势》研究](docs/rust-panel-advantages.md)。首批安全修复已实施，其余生产准入项仍待开发和验收。
