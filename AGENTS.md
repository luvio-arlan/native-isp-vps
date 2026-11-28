# Repository Engineering Guidelines (AGENTS.md)

本仓库遵循 **AI-Native Garden** 工程规范：所有可静态拦截的不变量均已固化在 `scripts/lint-arch.mjs` 与 Rust 单测中，提交前务必执行唯一质量门禁：

```bash
bash scripts/check.sh
```

## 模块地图（`app/src-tauri/src/`，单一职责）

| 模块 | 职责 |
| --- | --- |
| `lib.rs` | 仅做 Tauri 装配（插件、命令注册、退出落盘） |
| `state.rs` | 全局状态与可见窗口定向推送 `emit_to_visible_windows` |
| `poller.rs` | 分层轮询（2s 可见 / 15s 收起）、1Hz 托盘流式刷新、收件箱监听 |
| `commands.rs` | IPC 命令收口（参数校验、阻塞 I/O 走 `spawn_blocking`） |
| `validate.rs` | 所有外部输入（交接 JSON / 手动表单 / 旧 nodes.json）的校验 |
| `ssh.rs` | **唯一** SSH 进程构建器（`BatchMode` + `--` 终止选项 + 二次校验） |
| `panel_tunnel.rs` | 3x-ui 面板 API 的 SSH `-L` 加密隧道（fail-closed，防孤儿进程） |
| `xui.rs` | 3x-ui API 遥测（Token 优先 + Cookie 兜底、分层缓存） |
| `local_probe.rs` | 真实 RTT（macOS `IP_BOUND_IF` 绕 TUN）、AI 端点测速、防泄露验真 |
| `carrier_probe.rs` / `ssh_audit.rs` | 零 Agent 三网回程探测 / 内核 + DNS 只读体检（复用 ControlMaster） |
| `mihomo.rs` | 本机 Clash Verge Rev / Mihomo Unix Socket 防护透视与 `/traffic` 流 |
| `metrics.rs` | 健康分（平滑曲线 + 7 点中位数 + EWMA）与 24h 多分辨率历史 |
| `storage.rs` | `0600` 原子写入的节点 / 凭据 / 设置 / 历史持久化 |

前端：`app/src/lib/tauri.ts` 是唯一 IPC 门面；领域逻辑放 `app/src/lib/`（`quota.ts` 配额口径、`format.ts` 周期与格式化、`diagnostic.ts` AI 诊断包、`theme.ts` 主题、`clipboard.ts`）。

## 核心不变量（`lint-arch.mjs` / Rust 单测强拦截）

1. **零阻塞热路径 + 可见窗口定向推送**：`carrier_probe.rs` / `local_probe.rs` / `mihomo.rs` / `panel_tunnel.rs` / `poller.rs` / `inbox.rs` 严禁 `std::fs::`；周期性 `cockpit://telemetry` 与 `cockpit://carrier-quality-updated` 必须走 `state::emit_to_visible_windows`。
2. **凭据与传输安全**：
   - 严禁引入 `keyring` / macOS Keychain API；凭据统一由 `storage.rs` 以 `0600` 原子写入 `~/.config/native-isp-vps/secrets.json` 并走 `OnceLock` 内存缓存。
   - 面板 API 必须经 `panel_tunnel::resolve` 取路由：隧道曾可用而中断时返回 `Paused`，**绝不**把 Token 降级为明文。
   - 只允许 `ssh.rs` 调用 `Command::new("ssh")`（`SSH_BUILDER_ONLY`）；新增外部字段必须先过 `validate.rs`。
   - `tauri.conf.json` 必须保留严格 CSP（`CSP_REQUIRED`）；capability 只授予 `app/src` 实际调用的权限。
3. **IPC / 版本 / 文档契约**：
   - 前端仅允许在 `app/src/lib/tauri.ts` 导入 `@tauri-apps/api/core`；命令双向一致（`IPC_CONTRACT_PARITY` / `IPC_DEAD_COMMAND`），前端订阅的事件必须有 Rust 发射方（`EVENT_CONTRACT_PARITY`）。
   - `package.json` / `Cargo.toml` / `tauri.conf.json` 版本一致，且 `CHANGELOG.md` 必须有对应 `## vX.Y.Z` 段落（`CHANGELOG_PARITY`）。
   - 零 TypeScript 逃生舱（禁止 `as any` / `as never` / `@ts-ignore`），单源码文件 `<= 800` 行；Rust MSRV 为 `rust-version = "1.77"`，勿用更新的 std API。
4. **测速与代理性能口径**：
   - RTT：macOS 绑定物理网卡握手面板/订阅 HTTP 端口（不碰 Reality 端口），其余平台走 TUN 自适应回退；AI 端点测速采用 IPv4 优先 + 连接池 + HTTP/2 `HEAD` 预热 + 双连发取最小值；三网探测复用 SSH `ControlMaster`（60s 可见 / 300s 隐藏，零 VPS 常驻进程）。
   - `references/mihomo-rules.yaml` 中所有 `GEOIP` 规则必须携带 `,no-resolve`；SKILL / references 中的 `resolv.conf` 模板禁止 `rotate`（`NO_RESOLV_ROTATE`）。

## 操作清单

- **新增 IPC 命令**：`commands.rs` 写 `#[tauri::command]` → `lib.rs` 的 `generate_handler![]` 注册 → `app/src/lib/tauri.ts` 的 `api` 增加类型化方法 → 类型放 `app/src/types/telemetry.ts`（Rust `serde(rename_all = "camelCase")` 对齐）。
- **新增体检项**：在 `ssh_audit.rs` 的 `AUDIT_SCRIPT` 输出 `KEY=value`，在 `build_items` 增加 `AuditItem` 并补单测；前端与 AI 诊断包自动渲染未通过项。
- **发版**：同步三处版本号 → 在 `CHANGELOG.md` 顶部写 `## vX.Y.Z — 日期` → `bash scripts/check.sh` → 提交并推送 `vX.Y.Z` tag，`release-app.yml` 自动构建签名包与 updater `latest.json`。
