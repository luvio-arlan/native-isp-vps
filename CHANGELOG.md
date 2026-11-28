# Changelog

All notable changes to Native ISP VPS & Cockpit. The release workflow publishes the section
matching the pushed tag (`## vX.Y.Z`) as the GitHub Release body (`lint-arch` enforces parity).

## v0.2.0 — 2026-11-28

### 🌐 网络（根因级修复）
- **修复 REALITY 新连接间歇 +2s 握手卡顿**：实测部分机房屏蔽 `1.0.0.1`，而旧版 Stage 1 写入的 `resolv.conf`（`1.1.1.1 / 8.8.8.8 / 1.0.0.1` + `rotate` + `timeout:2`）会让约 1/3 的解析先撞上死节点白等 2 秒；Xray REALITY 对**每条新连接**都会解析一次伪装目标，于是 1/3 的代理握手凭空慢 2 秒。SKILL.md 改为「先逐个探测上游、只写入可达者、去掉 rotate、`timeout:1 attempts:2`」，Xray 内置 DNS 同步只用已验证上游。
- **内核深度体检新增「DNS 上游可达性」**：逐个探测 VPS `resolv.conf` 上游（python3 / dig，零常驻进程），不可达即判为严重项并进入「AI 修复诊断包」，自愈闭环可自动发现此类问题。
- **真实 RTT 测量（macOS）**：探测 socket 通过 `IP_BOUND_IF` 绑定物理网卡，绕过 Clash/Mihomo TUN 的本地 SYN-ACK 应答，优先握手面板/订阅 HTTP 端口；不再每 2 秒向面板发 `HEAD`，也不再触碰 Reality 端口（每次 accept 都会让 Xray 回源伪装站）。

### 🔒 安全
- **面板 API 全程 SSH 加密**：旧版每 2 秒把 3x-ui 管理员 Bearer Token（以及登录兜底时的密码）经 `http://` 明文发过公网。现在只要节点配置了 SSH 体检私钥，看板自动建立专用 `ssh -L` 隧道访问 `127.0.0.1` 面板；隧道曾经可用而临时中断时**宁可暂停也绝不降级明文**（fail-closed），总览页实时显示「面板 API：SSH 加密隧道 / HTTPS / 明文 ⚠」。隧道子进程以 stdin 管道绑定 App 生命周期，崩溃也不会遗留孤儿 ssh。
- **修复 SSH 参数注入**：交接 JSON 中的 `sshUser` 若为 `-oProxyCommand=...` 可被 ssh 解析为选项。新增统一 `ssh.rs` 构建器（`--` 终止选项解析 + 主机/用户名二次校验），`lint-arch` 禁止在别处直接 `Command::new("ssh")`。
- **全字段输入校验**（`validate.rs`）：host / SSH 用户名 / URL 仅允许 `http(s)://` / 标签去控制字符并限长。
- **启用严格 CSP**（`script-src 'self'`）并按最小权限裁剪 WebView capability（移除未使用的窗口 show/hide/close/focus/minimize、通知、退出、文件定位权限）；移除未使用的前端通知插件依赖。
- **收件箱加固**：交接文件 64KB 上限；格式错误/校验失败的文件隔离为 `*.json.invalid` 并系统通知，不再每秒重复解析、也不再让含密钥的坏文件无人知晓；`cockpit-handoff.sh` 改为校验 JSON 后原子 `mv` 落盘。
- 依赖审计：npm 0 漏洞；Rust 在 macOS / Windows 发布目标上 0 个适用漏洞（剩余公告均为 Tauri 的 Linux GTK/D-Bus 传递依赖「停止维护」提示）。
- CI / Release 工作流最小权限（默认 `contents: read`，仅发布任务可写）、发布构建使用 `--frozen-lockfile`。

### ✨ 交互与稳定性
- 24h 趋势按**真实时间窗**截取（旧版按条数截取，应用停机后会显示 35h+ 数据），并下采样为 5 分钟粒度（1440 → 288 点，IPC 负载降 80%，悬停更稳）。
- RTT 图表不再把超时画成 0ms（旧版看起来像「延迟极佳」），丢包率按样本加权。
- 总览 / 状态栏 HUD / 用户页配额口径统一（`lib/quota.ts`）；重置日 29–31 号按真实月份长度计算周期与月耗预测（旧版一律压成 28 号并按固定 30 天推算）。
- 切换节点时丢弃上一个节点的迟到快照，不再在新节点标题下闪现旧数据；面板短暂失联时保留最近已知配额而非闪成 0 B。
- 重复执行 AI 交接只更新凭据，不再冲掉手动设置的配额 / 重置日 / 私钥路径，`createdAt` 保留。
- 主题在首帧前应用（消除启动时暗色 → 浅色闪烁）；剪贴板失败有明确提示；图标按钮补齐无障碍标签、节点列表支持键盘操作。
- 退出时可靠落盘（含未满一分钟的数据）并回收 SSH 隧道。

### 🤖 AI-Native 工程
- `lint-arch` 新增 `SSH_BUILDER_ONLY`、`IPC_DEAD_COMMAND`（双向 IPC 契约）、`EVENT_CONTRACT_PARITY`、`CSP_REQUIRED`、`NO_RESOLV_ROTATE`、`CHANGELOG_PARITY` 门禁；Rust 单测 7 → 20。
- AGENTS.md 补充模块地图与「新增 IPC 命令 / 发版」检查清单。

## v0.1.3 — 2026-11-27
- 共享配额默认值调整，AI-Native 门禁与版本一致性检查。
