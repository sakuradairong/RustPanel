# 安全密钥配置与升级

当前版本取消了 JWT 和 Cookie Session 的公开默认密钥。启动前必须提供两个独立随机值；缺失或不合法会在创建运行目录、数据库和 HTTP 监听前返回错误。

| 环境变量 | 格式与最低长度 | 用途 |
| --- | --- | --- |
| `RP_SECRET_KEY` | 至少 32 字节，无空白字符 | HS256 JWT 签名 |
| `RP_SESSION_KEY` | 至少 64 字节，无空白字符，与前者不同 | Actix Cookie Session |
| `RP_FILE_ROOTS` | 冒号分隔的绝对路径（可选） | 文件 API 受管根；未设置时默认为 `/www:/var/www` |
| `RP_FILE_EXPERT_MODE` | `1` / `true` / `yes` / `on`（可选） | 将 `/` 加入受管根；默认关闭 |

建议使用操作系统随机源生成 32/64 个随机字节后转为 hex（64/128 个字符）。最低长度校验不能保证随机性，不要使用重复字符、口令或示例字符串。曾公开的默认值即使显式配置也会被拒绝。启动错误仅包含变量名和要求，不输出密钥值。

`config/conf.yaml` 中的 `app.secret_key` 是历史配置字段，当前不是实际 JWT 密钥来源。仅修改它无法轮换 JWT。此批没有自动密钥生成、密钥文件读取或完整密钥轮换服务；部署者需将持久化密钥安全注入进程环境。

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

自 RP-01 起：新口令使用 Argon2id；数据库不再保存 `initial_password` 明文。旧 SHA-1 用户在登录成功后即时重哈希。首次安装时一次性管理员口令仅写入启动日志（内存生成），并标记 `must_change_password`。

自 RP-23 起：访问令牌只携带用户 id + `jti` + `token_version`，并在 `sessions` 表登记；登出或改密会立即使旧令牌失效。升级后需重新登录。`kid` 多密钥窗口与 refresh cookie 轮换仍待后续增强。

## 本批行为变化

- 所有 SSL 接口需要认证；签发、续期、上传、部署、删除、注册 ACME 账号和安装 Certbot 还需要数据库中的当前管理员角色。
- 每次认证检查数据库中的用户是否存在、状态是否正常、密码哈希是否仍匹配。返回当前数据库身份和角色，忽略 token 中的旧角色。
- 数据库缺失、查询失败或连接池耗尽返回 503；不凭有效签名直接放行，也不在连接池耗尽时 panic。
- 监控 SSE 在建立连接时使用同一认证检查；暂时保留 query token 兼容现有前端。一次性票据及已建立连接的周期撤销检查仍待 RP-05。
- `/user/me` 不再返回密码哈希。此批尚未移除旧 JWT 的密码字段，也未迁移 SHA-1 存储，不代表全部 P0 已完成。

自 RP-03 起：高风险写接口按权限码鉴权（admin/operator/viewer）。自 RP-06 起：文件 list/save/content 必须落在受管根内，拒绝 `..`、软链逃逸与根外绝对路径（403），写入使用同目录临时文件 + rename。

这些保护尚不覆盖宿主特权拆分（RP-07）、审计 intent/outbox（RP-09）和备份恢复 CLI。生产准入继续按开发文档的全部 M0 条件验收。
