# RustPanel 完整审查整改记录

整改基线：`dev@1a9cc4b1b6c125dff60d9bf8b72728e2957f1464`，2026-10-11。
原审查覆盖从 `bb14afd` 到该提交的实现，沿代码规范与开发规格两条线核对。
本文记录本轮修复的行为、验收证据和部署边界；产品整体路线仍以 [开发方案](development-plan.md) 为准。

## 问题与处理

| # | 原问题 | 当前行为及验收 |
| --- | --- | --- |
| 1 | 管理员更新/重置密码失败仍 COMMIT，删除忽略提交失败 | 用户创建、更新、删除、密码重置采用 Diesel immediate transaction；用户修改、角色分配、版本递增和会话撤销一起提交或回滚。真实 HTTP + SQLite trigger 验证中途失败，重复用户名仍返回 400。用户自助改密同样事务化，并在锁内重新核对会话与版本。 |
| 2 | 设置证书路径即可绕过公共明文绑定限制 | 当前 HTTP listener 只允许 loopback 或明确的明文 override；证书环境变量不能启用 TLS。实际启动子进程验证伪证书路径被拒绝。 |
| 3 | SSE 仅检查账号状态，登出/过期后仍可取票或推送 | 新迁移 008 将票据绑定 session hash、token_version、访问令牌 expiry。签发、消费和每 2 秒推送检查会话、账号、改密门禁、monitor:read。签发限额与单次消费事务化。真实 HTTP 验证登出拒绝剩余票据并结束已建立流；单元验证过期、改密、权限移除。 |
| 4 | 目录 fd 提前关闭，列表元数据读取失败 | BoundReadDir 持有目录句柄直到遍历结束。HTTP 测试断言实际文件名称/大小、子目录、软链名称；原中间目录替换边界测试继续通过。 |
| 5 | 原子保存将已有文件变成进程用户所有的 0600 | 从受管父 dirfd 无跟链获取目标元数据，临时文件发布前保持 uid/gid/mode；元数据设置失败则不发布。回归 0644/0755、所有者、软链；新文件保持 0600。Linux 下具备 CAP_CHOWN 时额外验证不同 uid/gid 的保存；当前沙箱不支持测试 UID/GID，最终在沙箱外验证该分支通过。 |
| 6 | 登录返回的原始 JWT cookie 未设置安全属性 | 登录 cookie 使用 Path=/、HttpOnly、SameSite=Lax 和部署对应的 Secure；登出删除同一路径 cookie。真实加密登录请求验证响应属性和删除 cookie。 |
| 7 | 初始密码进入普通滚动日志 | 凭据仅写入独占创建的进程用户 0600 文件；日志仅记录文件位置。父目录限制所有者与写权限；文件写入并 fsync 后才能提交管理员。 |
| 8 | 启动和备份接受未来版本或部分损坏迁移历史 | 二者共用完整历史校验：已知连续前缀及所有 checksum，备份必须是完整期望版本。回归未来版本、缺失记录、早期 checksum 篡改、不兼容旧表。 |
| 9 | 监听失败或角色/凭据交付失败留下不可登录管理员 | 实际 socket 先绑定，再迁移/初始化管理员；角色和凭据失败回滚。旧 panel.lock 写失败仅告警，避免让已经提交且凭据已交付的账号变成启动错误。回归端口占用及交付失败。 |
| 10 | 前端没有强制改密流程 | 登录、刷新、直接进入受保护路由均先读取 /user/me；需要改密时仅显示当前密码、新密码和确认表单。改密成功清除已撤销令牌并重新登录。Node VM 测试路由门禁、确认校验与成功清理。 |
| 11 | 前端登出只删除本地 token | 先 POST /login/logout，成功后清本地状态；服务端失败保留凭据并显示错误以便重试。交互测试覆盖成功/失败顺序。 |
| 12 | 文件页默认 /，与受管根冲突 | 新 /file/roots 返回配置与可用根，前端从可用根开始，支持自定义根选择；面包屑不允许跳到非受管祖先。没有可用目录时显示空状态。 |
| 13 | 虚构管理员菜单/权限，viewer 看到写操作 | /user/me、/user/menus 返回当前数据库角色和权限；菜单、路由及写按钮按权限显示，服务端仍独立鉴权。支持管理 operator/viewer，并保留 user 的 viewer 映射。HTTP 验证角色变更立即反映，前端验证 viewer/operator 控制显示。 |
| 14 | pages.js 存在重复列表/弹窗代码 | 保留为后续维护改进。本轮用统一 data-permission 机制处理操作显示，避免为消除代码重复扩大功能改动；完整页面模块拆分单独推进。 |
| 15 | 已执行但结果写失败的审计动作被错误标记 aborted | 陈旧 pending 标记 unknown，表示执行结果未知；不再推断动作未执行。现有结果/事件/outbox 事务保持，回归 unknown 状态。不能自动重跑外部动作。 |
| 16 | Docker exec 无限缓存输出且忽略退出码 | 最多缓存 1 MiB，整个响应 deadline 30 秒，检查 daemon exec exit_code；结构化返回 output/exit_code/truncated/timed_out/exec_id，界面保留错误输出并显示失败。输出上限与挂起流取消有单元回归。 |
| 17 | 日志大于 20 万行时 tail 指向旧位置 | 读取真实 EOF、先过滤再维护尾部队列；返回最多 10000 行和 4 MiB，超过 64 KiB 的单行明确报错，容量受限时报告 truncated。回归超过旧行数阈值、过滤及内存边界。 |
| 18 | 维护任务嵌套 checkout，max_pool=1 超时；投递阻塞异步线程 | 自取连接的审计辅助操作先执行，再持有清理连接；outbox 使用 spawn_blocking。max_pool=1 回归审计与票据清理。 |
| 19 | KEK 文件未经所有者/权限/软链检查 | 从 O_NOFOLLOW/O_NONBLOCK 打开的 fd 检查普通文件、进程用户所有、0600，读取上限 128 字节；回归 0644、软链、目录拒绝。 |

## 验证

验收使用实际临时 SQLite、Actix router、面板启动子进程和无第三方依赖的 Node VM 交互测试。
故障注入在已经修改用户行之后使版本递增或会话撤销失败，检查原密码、状态、角色、版本和会话不变。
所有测试只使用临时目录和测试密钥，不修改宿主服务、iptables 或真实 Docker 容器。

```sh
cargo test --locked --offline
node --test tests/frontend.test.cjs
node --check public/app.js
node --check public/pages.js
node --check public/ui.js
git -c core.whitespace=cr-at-eol diff --check
```

最终结果：`cargo test --locked --offline` 退出 0，**117 项 Rust 测试通过**；`node --test tests/frontend.test.cjs` 退出 0，**5 项交互测试通过**；三个 JavaScript 文件的语法检查及 CRLF 感知的 diff 检查通过。Rust 整套在沙箱外运行，文件所有者回归需要测试 UID/GID 支持。现有 security.rs 有一项未使用字段编译警告，不影响结果。原测试的首次失败和本轮开发中已纠正的编译/fixture 错误不作为通过证据。

## 部署与后续边界

1. 期望 schema 升至 8，只添加 `008_sse_sessions.sql`，不改已部署迁移的 checksum。旧未绑定票据被撤销，客户端重新取票；回退需恢复匹配的数据库快照。
2. 当前仍是 HTTP listener。HTTPS 采用 loopback + 可信 TLS 代理，设置 `RP_COOKIE_SECURE=1`；公开明文需要明确 override。进程内 TLS/ACME 面板证书继续按原计划实现。
3. 初始凭据默认位于 `runtime/bootstrap-admin.txt`；用面板进程用户读取，改密后删除。可设置 `RP_BOOTSTRAP_CREDENTIAL_FILE`，父目录必须可信且不可由其他用户写入。已经存在的文件不会覆盖；若首次事务提交失败而留下文件，应先确认数据库没有初始化账号，再由运营处理遗留文件。
4. Docker API 无单独停止 exec 的通用接口；超时释放面板 attachment，但容器内进程可能继续执行。需依据 exec_id 检查，不能将响应超时理解为进程已经被杀。本轮未连接真实 Docker daemon，真实容器退出码/超时行为还需部署环境验收。
5. 审计 unknown 状态保留不确定性；持久化执行结果的独立恢复队列仍可增强。独立 Unix socket runner、主进程降权、secret_ref 业务接入、refresh 轮换、SBOM/签名、浏览器端完整视觉验收和 pages.js 拆分继续按原路线推进。

本轮关闭功能与安全审查缺陷，不据此宣称所有 M0 生产准入条件已经完成。
