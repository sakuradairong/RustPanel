# 开发进度与验证记录

日期：2026-10-10。任务定义来自[开发方案](development-plan.md)。规划中的代码基线保留不变；本文件记录后续实施状态，避免把已有源码、设计目标和经过测试的行为混在一起。

## 首批：认证访问边界

| 任务 | 状态 | 本次实现 | 后续边界 |
| --- | --- | --- | --- |
| RP-04 SSL 敏感路由立即认证 | 已实现并验证 | 所有 SSL handler 认证；写操作暂用当前 admin 角色 | 资源范围 RBAC 属 RP-03 |
| RP-02 fail-closed / 禁用检查 | 已实现并验证 | 查询错误/池耗尽返回 503；禁用/不存在/改密拒绝；以 DB 角色授权 | 独立 session 撤销属 RP-23 |
| RP-25 消除公开密钥默认回退 | 已实现并验证 | 强制独立环境变量、长度/空白/旧默认值校验；监听前拒启 | 实例自动生成/受限文件加载和完整轮换仍需 bootstrap 扩展 |
| RP-05 SSE 一次性票据 | 部分 | 建连认证复用 DB 状态检查 | 一次性票据、活动连接撤销、URL 去长期 JWT 未实施 |
| RP-11 可复现依赖 | 部分 | 提交 Cargo.lock；约束 sysinfo/bollard；声明 Rust 1.88，CI --locked | 漏洞/许可扫描、SBOM、签名和双架构仍待实施 |

兼容变更和部署步骤见[安全密钥配置](security-bootstrap.md)。这批没有 schema 迁移，没有变更用户数据，没有执行真实 Certbot、Docker、防火墙或生产数据库操作。

## 第二批：RP-13 schema 迁移与 SQLite 热备

| 任务 | 状态 | 本次实现 | 后续边界 |
| --- | --- | --- | --- |
| RP-13 版本化 schema 迁移与 SQLite 一致性热备 | 已实现并验证 | `schema_migrations` 表；`001_initial` 创建 `rp_users`；空库迁移、旧库无 history 时基线采纳、checksum 不一致拒启；`VACUUM INTO` 热备后 `integrity_check` + 版本校验；启动走 `migrate_and_bootstrap` fail-closed | Argon2id/会话表（RP-01/RP-23）、权限表（RP-03）将作为后续有编号迁移加入；CLI `rp backup panel` 属 RP-43 |

要点：

- Schema 权威是 `schema_migrations`，不是 `panel.lock`。
- 空用户表且存在 `panel.lock` 时拒绝静默重新灌入管理员，避免误覆盖运维意图。
- 仍保留 SHA-1 口令与 `initial_password` 明文列；口令迁移属 RP-01，不在本批改写用户数据。

## 验证范围

真实 Actix 路由使用临时 SQLite 数据库验证以下行为：

- 12 个 SSL 路由无凭据返回 401；8 个 SSL 写路由普通用户返回 403。
- 数据库中的普通用户持有旧 admin token 仍不能访问 admin API 或 SSL 写接口。
- 当前身份正确返回，`/user/me` 不回显密码哈希；已认证的证书 provider 查询可用。
- 禁用、删除、密码变化、过期或无效 token 被拒绝；监控 SSE 也检查用户状态。
- 缺 DB pool、无用户表、连接池耗尽返回 503；请求不绕过认证、不 panic。
- 缺失/过短密钥时真实 `panel` 二进制拒绝启动，未创建 runtime 或数据库，错误不泄露输入密钥。
- 密钥纯校验测试覆盖长度、空白、重复和已公开值；登录查询回归验证正确密码也不能登录禁用账号。

RP-13 额外验证（临时文件 SQLite，无宿主服务）：

- 空库迁移到期望版本 1，重复执行幂等。
- 已有 `rp_users`、无 `schema_migrations` 的旧库被采纳且用户行保留。
- 人为篡改 migration checksum 后再次迁移被拒绝。
- `VACUUM INTO` 备份目标通过 `integrity_check` 与 schema 版本校验，可独立打开并读出用户；已存在目标路径拒绝覆盖。

构建和测试命令：`cargo test --locked --offline`；本批结果为 28 个测试全部通过（19 个库测试 + 9 个集成测试），既有站点/安装器与安全集成测试保留。系统级 TLS 部署、ACME 签发、完整浏览器回归和跨架构尚未验证。

## 接下来

1. RP-01 / RP-23：Argon2id、移除初始密码明文、JWT 最小身份、可撤销会话和登录限流（依赖本批迁移框架）。
2. RP-03 / RP-06 / RP-07 / RP-09：资源权限、路径边界、受限 runner 与审计。
3. RP-05 补齐一次性 SSE 票据；RP-11 补齐 audit/deny/SBOM/签名。

未完成全部 M0 前，不把已落地修复称为“生产安全已完成”。
