# Repository Engineering Guidelines (AGENTS.md)

本仓库遵循 **AI-Native Garden** 工程规范：能由静态门禁拦截的约束均已固化为代码检查，提交前务必执行唯一质量门禁：

```bash
bash scripts/check.sh
```

## 硬性架构约束（已由 `scripts/lint-arch.mjs` + CI 强制拦截）

1. **严禁引入 `keyring` / macOS Keychain API**：未签名 / ad-hoc macOS 构建无法持久化 Keychain ACL，会在轮询时触发无限弹窗。节点凭据统一由 `app/src-tauri/src/storage.rs` 以 `0600` 原子写入 `~/.config/native-isp-vps/secrets.json`，且所有读取必须走 `OnceLock<Mutex<HashMap<String, NodeSecrets>>>` 内存缓存（轮询热路径严禁直读磁盘或系统凭据库）。
2. **IPC 单一收口**：前端严禁在 `app/src/lib/tauri.ts` 之外直接导入 `@tauri-apps/api/core`。
3. **零类型逃生舱与文件体积上限**：禁止 `as any` / `as never` / `@ts-ignore` / `@ts-expect-error`；单源码文件不得超过 `800` 行。
4. **AI 延迟测速口径**：端到端 AI API 测速（`app/src-tauri/src/local_probe.rs`）必须采用 IPv4 优先 + HTTP/2 Keep-Alive `HEAD` 预热 + 复用连接双连发取最小值，禁止用冷建连单次 `GET` 汇报延迟。
