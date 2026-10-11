# 开发进度与验证记录

日期：2026-10-10（各批次）；本次文档重构 2026-10-11。**原始任务定义保存在历史[开发方案 v0.2](development-plan-v0.2.md)**（RP-01..RP-43 的原始拆分），**当前范围、优先级与阶段划分以新版[开发方案 v0.3](development-plan.md)为准**。规划中的代码基线保留不变；本文件记录后续实施状态，避免把已有源码、设计目标和经过测试的行为混在一起。

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

- Schema 期望版本：RP-23=**3**，RP-03=**4**，RP-09=**5**，RP-05=**6**，RP-08=**7**。
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
| RP-10 TLS/bootstrap/cookie/CSRF | 已实现并验证 | 非 loopback 明文绑定拒启（需显式 `RP_ALLOW_INSECURE_PUBLIC_BIND`，证书环境变量不能启用 TLS）；cookie `Secure`/`HttpOnly`/`SameSite=Lax`；cookie 写请求 Origin/Referer CSRF；`must_change_password` 门禁 + `POST /user/change-password`；示例 host `127.0.0.1` | 进程内 rustls 终结、ACME 面板证书、security_dir 轮换 UI 仍可增强 |

要点：测试累计 **63** 通过（44 库 + 19 集成）。

## 第十一批：RP-11 供应链门禁

| 任务 | 状态 | 本次实现 | 后续边界 |
| --- | --- | --- | --- |
| RP-11 依赖锁定与 cargo-deny | 已实现并验证（deny 门禁） | `deny.toml`：许可白名单、advisory 有期限忽略（rust-crypto / reqwest 0.11 树）、sources 仅 crates.io；CI `supply-chain` 任务固定 `cargo-deny` 0.20.2 并 `cargo deny check --deny warnings`；包 `license` 修正为 SPDX `Apache-2.0` | SBOM/签名/双架构属 RP-16；`multiple-versions=allow` 待 crypto/HTTP 栈清理后收紧；rust-crypto/reqwest 升级后撤销 advisory ignore |

要点：

- 本地验证：`cargo deny check --deny warnings` → advisories/bans/licenses/sources 均 ok。
- 既有 `cargo test --locked` / `--locked` build 门禁保留。

## 第十二批：RP-08 秘密信封加密

| 任务 | 状态 | 本次实现 | 后续边界 |
| --- | --- | --- | --- |
| RP-08 DEK/KEK secrets | 已实现并验证（存储原语） | 迁移 `007_secrets`：`secret_keys`/`secrets`；AES-256-GCM 信封（随机 DEK + KEK wrap）；`RP_MASTER_KEY`（64 hex）或 `RP_MASTER_KEY_FILE`/`/etc/rustpanel/master.key`；缺 KEK 拒启；put/get/rewrap；离线 DB 无明文单测 | 业务面（DB/ACME/S3）全面改走 `secret_ref`、备份导出与 KEK 演练 UI、API token 不可逆 hash 表属后续 |

要点：`cargo test --locked` **73** 通过（53 库 + 20 集成）；schema 期望版本 **7**。

## 第十三批：安全栈合并评审与修复（2026-10-10）

RP-01~RP-13 十一个 PR（#4~#14）按堆叠链逐个评审后合入 `dev`。评审发现两个必须修复的问题，已作为修复提交随对应 PR 合入：

| 发现 | 严重度 | 处置 |
| --- | --- | --- |
| RP-03：22 个 docker handler 与 process kill/list、log list/read 仅 `AuthUser`，viewer 可越权操作宿主 | BLOCKER | 修复提交补挂 `require_perm`（docker:read/write、process:read/write、log:read），新增 viewer 越权拒绝与放行回归测试 |
| RP-06：openat2 只做校验即丢弃 fd，后续按完整路径重开，仅叶子受 `O_NOFOLLOW` 保护，中间目录换软链仍可逃逸 | MAJOR | 修复提交改为 fd 锚定：`open_bound_file` 返回校验时打开的 fd（openat2 `RESOLVE_BENEATH\|NO_SYMLINKS` 优先，逐组件 `openat` 兜底）；目录列举经 `/proc/self/fd` 固定 inode；写入走父目录 fd 的 `openat`+`renameat` 并 fsync 父目录；新增中间目录交换回归测试 |

合并后验证：`cargo test --locked` **76** 通过（53 库 + 23 集成）；dev 树与本地验证树逐字节一致；`build`/`supply-chain` CI 通过。

### 评审遗留项（按优先级）

1. **撤销失败被吞**：`admin.rs`/`user.rs` 改密后 `let _ = sessions::bump_token_version/revoke_all_for_user`——撤销写失败时旧 JWT 仍有效但返回 200，应传播错误。
2. **登录限流内存无上限**：`rate_limit.rs` 对未认证端点的任意用户名/IP 建桶且不淘汰（DoS）；`clear_success` 清空整个 per-IP 桶；`realip_remote_addr()` 信任转发头。
3. **outbox 无投递 worker**：`audit/store.rs` 的 pending/retry 函数无生产调用方（「可重投」未兑现）；事件与 outbox 写入非同事务；结果写失败被吞、intent 停留 pending。
4. **行只增不减**：`sessions`/`sse_tickets`（含已消费/过期）/`audit_outbox` 无清理；`POST /monitor/tickets` 无限流。建议并入 RP-12 jobs。
5. **monitor SSE 阻塞调用**：`poll_next` 在异步线程做阻塞 `pool.get()`+SQLite 查询（每 10s/连接），应改 `web::block`/channel；前端 EventSource 重连会一直复用已消费票据（401 风暴），需重新取票。
6. **RBAC 双真相**：legacy `authority='admin'` 回退恒全权限；admin 更新用户与角色分配非一个事务；operator 的 `cert:issue/deploy` 因 ssl 路由要求 AdminUser 而不可达。
7. **RP-08 无业务调用方**：`secrets` 存储只有测试使用，ACME/TLS 私钥仍明文落盘（`ssl.rs`）；DEK/KEK 明文无 zeroize；`decrypt` 不比对 `key_id`。
8. **杂项**：`verify_sha1` 与 token 口令比较非常数时间（改 `subtle`）；`log.rs read_log` 仍在边界层外接受任意绝对路径；CSRF 中间件因无 cookie 认证流而近乎空转（保留作纵深防御，引入 cookie 流前不得移除）；RP-07 特权隔离（socket runner/降权）与输出上限前置仍未做（已在「接下来」跟踪）；deny advisory ignore 未设期限、`multiple-versions=allow` 待收紧。


## 第十四批：遗留项修复（issues #15–#18、#21）

评审遗留项按 issue 逐个修复合入（每个独立 PR，均附回归测试）：

| Issue | 修复 | PR |
| --- | --- | --- |
| #15 撤销失败被吞 | 改密/重置/禁用/删除全部 \`BEGIN IMMEDIATE\` 事务化，撤销失败整体回滚返回 5xx；删除用户先清 sessions/user_roles（顺带修复 Diesel SQLite 外键强制下删除必 500 的既有 bug） | #22 |
| #16 限流内存无上限 | \`check()\` 零分配；两 map 上限 10k 桶（超限先清过期再按最久未失败淘汰）；per-IP 桶按账号归属清理，成功登录不再重置整个 IP 桶 | #23 |
| #17 outbox/清理 | \`complete_intent\` 事务化；维护任务（30s outbox 投递到面板日志 + 每小时对账/清理：session、票据、90 天审计行）；票据签发每用户 10 张上限（429）；\`audited.rs\`/\`file.rs\` 不再吞结果写错误 | #24 |
| #18 SSE 阻塞 | SSE 体改为 spawn 任务 + mpsc 通道，状态检查与 sysinfo 采集走 \`spawn_blocking\`；前端断线重新取票重连（指数退避、8 次上限） | #25 |
| #19 RBAC 单一真相 | 移除 legacy authority 回退（无角色行即无权限）；ssl 变更端点改走 \`cert:issue\`/\`cert:deploy\`（operator 权限首次可达）；update_user 事务性已在 #22 完成 | #27 |
| #21 杂项 | \`verify_sha1\` 常数时间比较（subtle）；\`read_log\` 纳入 managed-root 边界（log roots 默认 runtime/logs + /var/log，可用 RP_LOG_ROOTS 覆盖）+ 20 万行内存上限；CSRF wrong-Origin 拒绝测试；deny advisory ignore 全部加 2027-03-31 期限；备份校验 SQLite URI 路径编码 | #26 |

剩余：#20（secrets 接入业务面）——涉及部署时私钥物化等语义决策，实施计划见 issue #20 评论。

## 第十五批：PR #2 打捞（#28/#29/#30）

8 月的前端大分支（29 commit，标题仅覆盖首个）按拆解方案分三个 PR 落地，原 PR #2 关闭：

| 部分 | 内容 | PR |
| --- | --- | --- |
| A 后端小件 | SSE \`Content-Encoding: identity\`（修复 Compress 中间件缓冲事件流）、负载 1/5/15 统计、进程 PID 搜索 | #28 |
| B Docker 功能面 | create 扩展（端口/env/重启策略白名单/网络模式）、images/volumes prune、one-shot exec（\`docker:write\`） | #29 |
| C 前端主体 | ui.js 共享原语、仪表盘环形仪表、监控时序图、Docker 卡片网格+详情抽屉、应用商店版本选择、文件管理器、系统信息页、登录打磨；SSE 全部改票据流+断线重新取票；保留 Apache/Caddy 站点段 | #30 |

未打捞：WebSocket 交互终端（原实现纯 \`decode_jwt\` + JWT-in-query，不可救药）→ issue #31 按 RP-35 票据化方案重做；分支 AGENTS.md 重写丢弃。

## 接下来

1. #20 secrets 接入业务面（ACME/TLS 私钥改走 secrets 表，计划见 issue #20）。
2. #31 容器 WebSocket 终端票据化（RP-35 起步）。
2. RP-07 后续：socket runner / 降权部署。
3. RP-23 增强：refresh 轮换与会话过期清理任务。
4. RP-11 后续：SBOM、签名、依赖重复版本收紧；rust-crypto/reqwest 升级后撤销 deny ignore。
5. 将 DB/ACME/S3 凭证接入 RP-08 `secrets` 表。

未完成全部 M0 前，不把已落地修复称为“生产安全已完成”。

## 2026-10-11 审查整改

以 `1a9cc4b` 为整改基线，逐项修复事务回滚、文件句柄与元数据、明文绑定绕过、登录 cookie、首次改密/登出、会话绑定 SSE、首启凭据交付、迁移历史、权限展示、日志 tail、审计未知状态、维护任务连接池和 KEK 文件权限。新迁移 `008_sse_sessions` 将期望 schema 升至 8，不修改历史迁移。

完整问题映射、验证方法、部署影响与剩余边界见 [审查整改记录](review-remediation.md)。界面结构拆分属于后续维护改进；独立 runner、secret_ref 业务接入、refresh 轮换、SBOM/签名仍按原计划推进，本轮不将 M0 标为完成。最终 Rust 全套 117 项、Node 交互测试 5 项通过；JS 语法与差异检查通过。

## 2026-10-11 文档重构记录

将 `docs/development-plan.md` 重构为 v0.3：原 v0.2 全文（约 1120 行）原样保留于 [development-plan-v0.2.md](development-plan-v0.2.md)，仅在顶部加历史定位，RP-01..RP-43 编号与原始需求保留；新版按 CP0–CP3 重排并把旧计划作为历史证据引用，范围调整与验收条件在当前切片中追踪。新增 [rust-panel-advantages.md](rust-panel-advantages.md) 研究稿，README 增加研究链接并区分现状与规划。本轮为纯文档改动，**未新增运行验收、未启动面板**；运行结论仍以前文批次与 [整改记录](review-remediation.md) 为准。补齐 RAII 析构/子进程管理和无 GC 的资源边界，收紧任务恢复、备份一致性、离线维护与跨产品测量契约；README 改为当前交付路线，避免把历史愿景误读为已实现能力。

## 2026-10-11 CI 配置准备修复

最新 PR #29、#30 的检查通过；失败的是 `dev` 推送运行 [38099427556](https://github.com/sakuradairong/RustPanel/actions/runs/38099427556)（`46b6281`）和 [38101025056](https://github.com/sakuradairong/RustPanel/actions/runs/38101025056)（`30ad06e`）。二者构建与供应链检查均成功，`bootstrap_csrf` 的两个测试失败：登录/登出 Cookie 路径读取被忽略的 `config/conf.yaml`，干净检出中缺少此文件，首次初始化 panic 后另一个测试报 `Once panicked`。本地已有配置掩盖了依赖，文档提交只再次触发同一失败。

在临时空目录运行同一测试程序，无配置时复现退出 101；复制已提交的配置样例后，5 项 Cookie/CSRF 测试全部通过。CI 新增 `Prepare test configuration`，在测试前从 `config/conf.yaml.example`、`config/log4rs.yaml.example` 生成运行配置，不引入部署凭据。按该 workflow 步骤在全新临时目录准备配置，再执行 `cargo test --locked --offline --no-run` 生成的所有测试程序，**117 项测试通过**。整套最终在沙箱外验证，因为沙箱不支持所有者回归所需的 UID/GID；首次沙箱内该项失败不作为通过证据。
