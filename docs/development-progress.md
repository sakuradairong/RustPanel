# 开发进度与验证记录

日期：2026-10-10。任务定义来自[开发方案](development-plan.md)。规划中的代码基线保留不变；本文件记录后续实施状态，避免把已有源码、设计目标和经过测试的行为混在一起。

## 首批：认证访问边界

| 任务 | 状态 | 本次实现 | 后续边界 |
| --- | --- | --- | --- |
| RP-04 SSL 敏感路由立即认证 | 已实现并验证 | 所有 SSL handler 认证；写操作暂用当前 admin 角色 | 资源范围 RBAC 属 RP-03 |
| RP-02 fail-closed / 禁用检查 | 已实现并验证 | 查询错误/池耗尽返回 503；禁用/不存在/改密拒绝；以 DB 角色授权 | 见 RP-23 会话撤销 |
| RP-25 消除公开密钥默认回退 | 已实现并验证 | 强制独立环境变量、长度/空白/旧默认值校验；监听前拒启 | 实例自动生成/受限文件加载和完整轮换仍需 bootstrap 扩展 |
| RP-05 SSE 一次性票据 | 已实现并验证（见第九批） | 建连认证复用 DB；一次性票据/禁 query JWT/周期撤销见第九批 | — |
| RP-11 可复现依赖 | 部分→本批补齐 deny 门禁 | 提交 Cargo.lock；Rust 1.88；CI `--locked`；`deny.toml` + CI `cargo deny check --deny warnings`（见第十一批） | SBOM、产物签名和双架构属 RP-16 / 后续 |

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

- Schema 期望版本：RP-23=**3**，RP-03=**4**，RP-09=**5**，RP-05=**6**。
- 旧版把 password 哈希放进 JWT `sub` 的令牌不再被接受，用户需重新登录。
- 访问令牌不再包含口令字段。

## 第五批：RP-03 资源范围 RBAC

| 任务 | 状态 | 本次实现 | 后续边界 |
| --- | --- | --- | --- |
| RP-03 资源范围 RBAC | 已实现并验证 | 迁移 `004_rbac`：`roles`/`permissions`/`role_permissions`/`user_roles`；内置 admin/operator/viewer；`require_perm` + `AdminUser`（`user:admin`）；installer/sites/firewall/file 写与 nginx 站点写受权限约束；新建用户默认 viewer；权限查 DB 即时生效 | Docker/process/service 写权限可继续收紧；资源实例级范围（按站点/路径）属后续增强 |

## 第六批：RP-06 文件路径边界与原子写

| 任务 | 状态 | 本次实现 | 后续边界 |
| --- | --- | --- | --- |
| RP-06 dirfd 文件边界与写保护 | 已实现并验证 | `path_bounds`：`RP_FILE_ROOTS` / `RP_FILE_EXPERT_MODE`；拒绝 `..`/根外绝对路径/软链；Linux `openat2(RESOLVE_BENEATH\|NO_SYMLINKS)` + dirfd `O_NOFOLLOW` 回退；file list/save/content 越界 403；同目录临时文件 + rename 原子写 | 上传/解压 zip-slip 与压缩炸弹属 C.9；写入经特权 runner 属 RP-07 |

要点：

- 未配置时默认受管根为 `/www` 与 `/var/www`（不是 `/`）；`RP_FILE_EXPERT_MODE=1` 才允许以 `/` 为根。
- 越界与软链逃逸返回 **403**（不再仅用 `contains("..")` 的 400）。

## 第七批：RP-07 受限特权 runner

| 任务 | 状态 | 本次实现 | 后续边界 |
| --- | --- | --- | --- |
| RP-07 受限 runner 与命令执行器 | 已实现并验证 | `src/privileged/`：动作白名单（iptables/ufw/systemctl）；参数校验拒 shell 元字符；`Command::new`+argv；超时+进程组 SIGKILL；输出上限；firewall 与 service 写路径经 runner；`operation_id` 追踪 | 独立二进制 + Unix socket/SO_PEERCRED、主进程降权与 Docker socket 隔离属部署/后续切片；installer/webserver/ssl 命令面可继续迁入 |

要点：

- 注入用例（`;`、`$()`、反引号、换行）在 runner 边界拒绝，不进入 shell。
- job_id 可空；M1 队列（RP-12）再接入。审计 intent 持久化属 RP-09。

## 验证范围

- 既有安全集成测试保留并改为会话 JWT：无凭据 SSL 401、角色 403、禁用/`token_version`/删除用户拒绝、登出后立即失效、池耗尽 503、缺密钥拒启。
- Argon2id / SHA-1 重哈希单测；登录限流单测；迁移到版本 4 + 热备校验。
- 断言访问令牌字符串不含口令哈希。
- Viewer 对 installer install、nginx 建站、firewall 规则写、file save 返回 403。
- 文件路径：`..`/根外路径/软链逃逸 403；受管根内 list 与原子写单测通过。
- Runner：参数注入拒绝；超时杀进程；firewall HTTP 注入 403。
- 审计：intent 落库后才执行；params 脱敏；outbox 可 delivered/retry。

构建和测试命令：`cargo test --locked`；本批结果为 55 个测试全部通过（39 个库测试 + 16 个集成测试）。系统级 TLS、ACME、浏览器回归和跨架构尚未验证。

## 第八批：RP-09 审计 intent / outbox / 脱敏

| 任务 | 状态 | 本次实现 | 后续边界 |
| --- | --- | --- | --- |
| RP-09 审计 intent/outbox/脱敏 | 已实现并验证 | 迁移 `005_audit`：`audit_intents`/`audit_events`/`audit_outbox`；高危动作先持久化 intent，失败则 503 拒绝执行；params/detail 脱敏；结果入 outbox 可重投；firewall/service 经 `execute_audited`，file save 挂 intent | 异步投递 worker、installer/ssl/docker 全覆盖、PTY 会话录制属后续 |

## 第九批：RP-05 一次性 SSE 票据

| 任务 | 状态 | 本次实现 | 后续边界 |
| --- | --- | --- | --- |
| RP-05 一次性 SSE 票据与连接撤销 | 已实现并验证 | 迁移 `006_sse_tickets`；`POST /monitor/tickets` 签发 ≤60s 一次性 stream 绑定票据；`GET /monitor?ticket=` 原子兑换；拒绝 `?token=` 长期 JWT；Bearer 头仍可用；流内周期校验账号禁用并关闭；前端改走票据 | 慢客户端背压/连接配额属后续；jobs SSE 可复用同一表 |

## 验证范围（累计）

- 既有安全集成测试 + file_bounds / privileged_runner / 审计单测。
- SSE：query JWT 401；票据一次性；错 stream 拒绝；兑换后复用 401。

构建和测试命令：`cargo test --locked`；本批结果见各后续批次。系统级 TLS、ACME、浏览器回归和跨架构尚未验证。

## 第十批：RP-10 TLS / bootstrap / CSRF

| 任务 | 状态 | 本次实现 | 后续边界 |
| --- | --- | --- | --- |
| RP-10 TLS/bootstrap/cookie/CSRF | 已实现并验证 | 非 loopback 明文绑定拒启（需 `RP_TLS_CERT`/`RP_TLS_KEY` 或 `RP_ALLOW_INSECURE_PUBLIC_BIND`）；cookie `Secure`/`HttpOnly`/`SameSite=Lax`；cookie 写请求 Origin/Referer CSRF；`must_change_password` 门禁 + `POST /user/change-password`；示例 host `127.0.0.1` | 进程内 rustls 终结、ACME 面板证书、security_dir 轮换 UI 仍可增强 |

要点：测试累计 **63** 通过（44 库 + 19 集成）。

## 第十一批：RP-11 供应链门禁

| 任务 | 状态 | 本次实现 | 后续边界 |
| --- | --- | --- | --- |
| RP-11 依赖锁定与 cargo-deny | 已实现并验证（deny 门禁） | `deny.toml`：许可白名单、advisory 有期限忽略（rust-crypto / reqwest 0.11 树）、sources 仅 crates.io；CI `supply-chain` 任务固定 `cargo-deny` 0.20.2 并 `cargo deny check --deny warnings`；包 `license` 修正为 SPDX `Apache-2.0` | SBOM/签名/双架构属 RP-16；`multiple-versions=allow` 待 crypto/HTTP 栈清理后收紧；rust-crypto/reqwest 升级后撤销 advisory ignore |

要点：

- 本地验证：`cargo deny check --deny warnings` → advisories/bans/licenses/sources 均 ok。
- 既有 `cargo test --locked` / `--locked` build 门禁保留。

## 接下来

1. RP-08 秘密存储（DEK/KEK 信封加密）。
2. RP-07 后续：socket runner / 降权部署。
3. RP-23 增强：refresh 轮换与会话过期清理任务。
4. RP-11 后续：SBOM、签名、依赖重复版本收紧。

未完成全部 M0 前，不把已落地修复称为“生产安全已完成”。
