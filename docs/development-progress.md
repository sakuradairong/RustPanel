# 开发进度与验证记录

日期：2026-10-10。任务定义来自[开发方案](development-plan.md)。规划中的代码基线保留不变；本文件记录后续实施状态，避免把已有源码、设计目标和经过测试的行为混在一起。

## 首批：认证访问边界

| 任务 | 状态 | 本次实现 | 后续边界 |
| --- | --- | --- | --- |
| RP-04 SSL 敏感路由立即认证 | 已实现并验证 | 所有 SSL handler 认证；写操作暂用当前 admin 角色 | 资源范围 RBAC 属 RP-03 |
| RP-02 fail-closed / 禁用检查 | 已实现并验证 | 查询错误/池耗尽返回 503；禁用/不存在/改密拒绝；以 DB 角色授权 | 见 RP-23 会话撤销 |
| RP-25 消除公开密钥默认回退 | 已实现并验证 | 强制独立环境变量、长度/空白/旧默认值校验；监听前拒启 | 实例自动生成/受限文件加载和完整轮换仍需 bootstrap 扩展 |
| RP-05 SSE 一次性票据 | 部分 | 建连认证复用 DB 状态检查与会话语义 | 一次性票据、活动连接周期撤销、URL 去长期 JWT 未实施 |
| RP-11 可复现依赖 | 部分 | 提交 Cargo.lock；约束 sysinfo/bollard；声明 Rust 1.88，CI --locked | 漏洞/许可扫描、SBOM、签名和双架构仍待实施 |

兼容变更和部署步骤见[安全密钥配置](security-bootstrap.md)。

## 第二批：RP-13 schema 迁移与 SQLite 热备

| 任务 | 状态 | 本次实现 | 后续边界 |
| --- | --- | --- | --- |
| RP-13 版本化 schema 迁移与 SQLite 一致性热备 | 已实现并验证 | `schema_migrations`；空库/旧库采纳；checksum 拒启；`VACUUM INTO` 热备校验 | CLI `rp backup panel` 属 RP-43 |

## 第三批：RP-01 Argon2id 与移除明文初始密码

| 任务 | 状态 | 本次实现 | 后续边界 |
| --- | --- | --- | --- |
| RP-01 Argon2id 与旧用户迁移 | 已实现并验证 | 迁移 `002`；Argon2id；SHA-1 登录即时重哈希；删除 `initial_password`；一次性口令不落库 | 前端强制改密 UX 可继续收紧 |

## 第四批：RP-23 可撤销会话与登录限流

| 任务 | 状态 | 本次实现 | 后续边界 |
| --- | --- | --- | --- |
| RP-23 sessions / 撤销 / 限流 | 已实现并验证 | 迁移 `003`：`sessions` 表 + `token_version`；JWT `sub` 仅 user id，含 `jti`/`iat`/`exp`/`ver`；登录建会话；`/login/logout` 撤销；改密/重置 bump version 并撤销全部会话；账号+IP 登录失败限流 | refresh cookie 轮换、CSRF、kid 多密钥窗口、会话清理定时任务仍可增强；RP-05 SSE 票据未做 |

要点：

- Schema 期望版本现为 **3**。
- 旧版把 password 哈希放进 JWT `sub` 的令牌不再被接受，用户需重新登录。
- 访问令牌不再包含口令字段。

## 验证范围

- 既有安全集成测试保留并改为会话 JWT：无凭据 SSL 401、角色 403、禁用/`token_version`/删除用户拒绝、登出后立即失效、池耗尽 503、缺密钥拒启。
- Argon2id / SHA-1 重哈希单测；登录限流单测；迁移到版本 3 + 热备校验。
- 断言访问令牌字符串不含口令哈希。

构建和测试命令：`cargo test --locked`；本批结果为 35 个测试全部通过（24 个库测试 + 11 个集成测试）。系统级 TLS、ACME、浏览器回归和跨架构尚未验证。

## 接下来

1. RP-03 / RP-06 / RP-07 / RP-09：资源权限、路径边界、受限 runner 与审计。
2. RP-05 补齐一次性 SSE 票据；RP-10 TLS/bootstrap/CSRF；RP-11 audit/deny/SBOM。
3. RP-23 增强：refresh 轮换与会话过期清理任务。

未完成全部 M0 前，不把已落地修复称为“生产安全已完成”。
