# 安全密钥配置与升级

当前版本取消了 JWT 和 Cookie Session 的公开默认密钥。启动前必须提供独立的签名密钥、会话密钥，以及秘密信封用的主密钥（KEK）；缺失或不合法会在创建运行目录、数据库和 HTTP 监听前返回错误。

| 环境变量 | 格式与最低长度 | 用途 |
| --- | --- | --- |
| `RP_SECRET_KEY` | 至少 32 字节，无空白字符 | HS256 JWT 签名 |
| `RP_SESSION_KEY` | 至少 64 字节，无空白字符，与前者不同 | Actix Cookie Session |
| `RP_MASTER_KEY` | 恰好 64 个十六进制字符（32 字节），无空白，非全零 | 秘密信封加密 KEK（RP-08）；也可用 `RP_MASTER_KEY_FILE` 或默认 `/etc/rustpanel/master.key`（必须为进程用户拥有的普通文件，0600，拒绝软链；hex 或原始 32 字节） |
| `RP_MASTER_KEY_ID` | 可选字符串 | 覆盖自动派生的 `key_id`（默认 `kek-` + SHA-256 前缀） |
| `RP_FILE_ROOTS` | 冒号分隔的绝对路径（可选） | 文件 API 受管根；未设置时默认为 `/www:/var/www` |
| `RP_FILE_EXPERT_MODE` | `1` / `true` / `yes` / `on`（可选） | 将 `/` 加入受管根；默认关闭 |

建议使用操作系统随机源生成 32/64 个随机字节后转为 hex（64/128 个字符）。最低长度校验不能保证随机性，不要使用重复字符、口令或示例字符串。曾公开的默认值即使显式配置也会被拒绝。启动错误仅包含变量名和要求，不输出密钥值。

`config/conf.yaml` 中的 `app.secret_key` 是历史配置字段，当前不是实际 JWT 密钥来源。仅修改它无法轮换 JWT。JWT/Session 密钥仍需经环境变量注入；`RP_MASTER_KEY` 也可改为读取 `RP_MASTER_KEY_FILE` 或 `/etc/rustpanel/master.key`（必须 0600，所有者为运行面板的用户）。此批提供 `rewrap_all` 原语，但没有面向运营的自动轮换 UI；丢失 KEK 则库内密文不可恢复，备份须与数据库分开保管。

## Linux / systemd

以下是部署步骤，不是本次已对宿主执行的操作。用 root 在目标服务器执行一次生成脚本；文件已存在时拒绝覆盖。不要每次启动重新生成密钥。

```python
import os
from pathlib import Path
import secrets

key_dir = Path('/etc/rustpanel')
key_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
key_file = key_dir / 'security.env'
content = (
    f'RP_SECRET_KEY={secrets.token_hex(32)}\n'
    f'RP_SESSION_KEY={secrets.token_hex(64)}\n'
    f'RP_MASTER_KEY={secrets.token_hex(32)}\n'
).encode()
fd = os.open(key_file, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
with os.fdopen(fd, 'wb') as stream:
    stream.write(content)
    stream.flush()
    os.fsync(stream.fileno())
```

在对应服务的 drop-in 加入（仓库脚本使用 `RustPanel.service`，实际名称以部署为准）：

```ini
[Service]
EnvironmentFile=/etc/rustpanel/security.env
```

保存后重新加载 systemd 配置并重启对应服务。确保环境文件由 root 拥有、权限 0600，父目录不允许非授权用户写入；不要把文件提交 Git、打印进日志或放进 Web 静态目录。备份文件应加密，并与数据库备份分开管理访问权。

本地开发也使用实例专用的环境文件。仅在受控 shell 中加载该文件并运行 `panel`，不要把真实密钥写在命令行参数中；不要复用生产密钥。构建与测试不需要生产密钥，测试使用进程内固定的测试专用值。

## 从旧版本升级

1. 在首次启动新版前生成并注入新的独立密钥；否则启动会拒绝继续。
2. 备份现有数据库、配置和受限密钥文件。自 RP-13 起启动会写入 `schema_migrations`；旧库若已有 `rp_users` 会在不丢用户数据的前提下登记基线版本。可用库内 `repository::backup::backup_database`（`VACUUM INTO` + `integrity_check`）做一致性快照，不要只复制正在写入的 `data.db3` 而忽略 WAL。
3. 替换产物并重新启动，确认服务可用，所有使用旧密钥的 JWT 失效，用户需重新登录。若 migration checksum 与当前二进制不一致，面板会拒绝启动。
4. 不要将旧公开密钥设进环境变量来恢复会话。回退也不能重新暴露未认证的 SSL 写接口。回退到旧二进制时需使用匹配该版本的数据库快照。

密钥只在进程初始化时读取；改变环境文件后需重启。轮换 JWT key 会使旧 JWT 全部失效。

自 RP-01 起：新口令使用 Argon2id；数据库不再保存 `initial_password` 明文。旧 SHA-1 用户在登录成功后即时重哈希。首次安装时一次性管理员口令写入 `./runtime/bootstrap-admin.txt`（可通过 `RP_BOOTSTRAP_CREDENTIAL_FILE` 指定），并标记 `must_change_password`。文件以独占创建方式写入，权限 0600；父目录必须属于运行用户且不允许组或其他用户写入。日志只显示文件位置。用运行用户在本机读取凭据，登录后完成强制改密，再删除该文件；不要把它提交 Git 或放进静态目录。绑定策略和实际监听 socket 在创建账号前验证，因此监听失败不会留下未知密码的管理员。凭据写入或角色分配失败会回滚账号；已有凭据文件拒绝覆盖。

自 RP-23 起：访问令牌只携带用户 id + `jti` + `token_version`，并在 `sessions` 表登记；登出或改密会立即使旧令牌失效。升级后需重新登录。`kid` 多密钥窗口与 refresh cookie 轮换仍待后续增强。

## 当前行为

- 所有 SSL 接口需要认证；写操作使用当前数据库中的 `cert:issue` / `cert:deploy` 权限，允许具备相应权限的 operator。
- JWT 只携带用户 id、会话 jti、token_version 和时间信息。认证检查当前账号、版本和会话撤销/过期状态；`/user/me` 返回真实 roles / permissions，不返回密码材料。
- 数据库不可用时拒绝认证或授权；不凭签名直接放行。
- 监控流只接受一次性票据或 Authorization Bearer，拒绝 query JWT。票据绑定签发会话、密码版本和访问令牌过期时间；每 2 秒复查账号、改密门禁、会话和 `monitor:read`，任何失败结束流。
- 前端登录和页面刷新均检查强制改密标志；登出请求服务端撤销成功后才清除本地凭据。输入错误的当前密码返回 400，保留正常会话以便重试。

自 RP-03 起：高风险写接口按权限码鉴权（admin/operator/viewer）。自 RP-06 起：文件 list/save/content 必须落在受管根内，拒绝 `..`、软链逃逸与根外绝对路径（403），写入使用同目录临时文件 + rename。

自 RP-07 起：firewall / systemd service 写操作经 `privileged` runner（动作白名单、参数校验、超时）。部署上建议主进程以非 root 用户运行、不加入 docker 组、不挂载宿主 Docker socket；独立 Unix-socket runner 仍为后续部署目标。

自 RP-09 起：schema 版本曾为 **5**；高危写（firewall/service/file save）执行前必须写入 `audit_intents`，失败则拒绝执行（503）；结果进 `audit_events` + `audit_outbox`（可重投）；日志字段脱敏 password/token/secret/authorization/content。

自 RP-05 起：schema 版本 **6**；监控 SSE 使用 `POST /api/v1/monitor/tickets` 签发的一次性短票据（`?ticket=`），禁止把长期 JWT 放进 EventSource URL；当前迁移 008 进一步绑定会话并周期检测撤销、过期、改密与权限变化。

自 RP-10 起：默认示例监听 `127.0.0.1`；向非 loopback 明文绑定会拒启，除非显式设置 `RP_ALLOW_INSECURE_PUBLIC_BIND=1`。当前监听器只有 HTTP，`RP_TLS_CERT` / `RP_TLS_KEY` 不会启用 TLS，也不能绕过绑定限制。生产 HTTPS 使用 loopback 后端与 TLS 反向代理，并显式设置 `RP_COOKIE_SECURE=1`。会话 cookie 使用 `HttpOnly` + `SameSite=Lax`，并在非 loopback/TLS 场景推荐 `Secure`（可用 `RP_COOKIE_SECURE` 覆盖）。带 session cookie 的写请求需匹配的 `Origin`/`Referer`。首启 `must_change_password` 用户除 `/user/me`、改密、登出外返回 403；改密接口为 `POST /api/v1/user/change-password`。

自 RP-08 起：schema 版本 **7**；面板秘密使用 AES-256-GCM 信封加密入库（每秘密随机 DEK，KEK 包裹 DEK）。**丢失 KEK 则密文不可恢复**；KEK 备份须与数据库备份分开保管并做过恢复演练。业务模块尚未全部改为 `secret_ref`；本批提供存储原语与启动门禁。

当前 schema 版本为 **8**。迁移 008 会撤销旧的未绑定会话的 SSE 票据；客户端重新取票即可。启动与备份验证检查整个迁移历史的连续性和每项 checksum，拒绝未来版本、历史缺口和被修改的早期记录。回滚二进制需同时恢复对应数据库快照，不能让旧二进制直接打开新版库。

审计结果保存失败后，陈旧 pending intent 被标记为 `unknown`，表示执行结果不确定，不能推断动作未执行，也不能自动重跑。维护任务兼容 max_pool=1，outbox 投递移到阻塞线程池。

日志读取到真实 EOF 后取尾部，最多返回 10000 行和 4 MiB；超过 64 KiB 的单行明确报错。Docker one-shot exec 最多缓存 1 MiB，整个响应有 30 秒 deadline，返回真实 exit_code、truncated、timed_out 和 exec_id；超时会释放面板连接，但 Docker API 无法单独终止 exec 进程，它可能仍在容器内运行，需按 exec_id 检查，不能自动重试。

这些保护尚不覆盖备份恢复 CLI、进程内 rustls 终结与全部宿主命令面迁入 runner。生产准入继续按开发文档的全部 M0 条件验收。
