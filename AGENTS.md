# Repository Engineering Guidelines (AGENTS.md)

本仓库遵循 **AI-Native Garden** 工程规范：所有可静态拦截的不变量均已固化在 `scripts/lint-arch.mjs` 与 Rust 单测中，提交前务必执行唯一质量门禁：

```bash
bash scripts/check.sh
```

## 模块边界与核心不变量

1. **后端单一职责分层（`app/src-tauri/src/`）**：
   - `lib.rs` 仅做 Tauri 装配；状态与可见窗口定向推送在 `state.rs`；分层轮询与 1Hz 流式托盘更新在 `poller.rs`；健康分与 24h 多分辨率历史在 `metrics.rs`；IPC 命令收口在 `commands.rs`。
2. **零阻塞热路径 + 可见窗口定向推送（`lint-arch.mjs` 强拦截）**：
   - `local_probe.rs` / `poller.rs` / `inbox.rs` 严禁 `std::fs::`；高频率 `cockpit://telemetry` 必须走 `state::emit_to_visible_windows`，禁止全局广播唤醒隐藏 WebView。
3. **凭据与 IPC 安全边界（`lint-arch.mjs` 强拦截）**：
   - 严禁引入 `keyring` / macOS Keychain API；凭据统一由 `storage.rs` 以 `0600` 原子写入 `~/.config/native-isp-vps/secrets.json` 并走 `OnceLock` 内存缓存。
   - 前端仅允许在 `app/src/lib/tauri.ts` 导入 `@tauri-apps/api/core`，且 `lint-arch.mjs` 自动校验前后端 IPC 命令契约一致性。
   - 零 TypeScript 逃生舱（禁止 `as any` / `as never` / `@ts-ignore`），单源码文件 `<= 800` 行。
4. **测速与代理性能口径（Rust 单测 + `lint-arch.mjs` 强拦截）**：
   - AI 端点测速（`local_probe.rs`）采用 IPv4 优先 + 复用连接池 + HTTP/2 Keep-Alive `HEAD` 预热 + 双连发取最小值；VPS `HEAD` 探针在冷建连/空闲后自动补发一次热探针消除伪抖动，健康分采用平滑曲线与 3 点中位数防悬崖跳分。
   - `references/mihomo-rules.yaml` 中所有 `GEOIP` 规则必须携带 `,no-resolve`，避免破坏 Fake-IP `<1ms` 秒发优势。
