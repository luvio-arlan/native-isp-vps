# Repository Engineering Guidelines (AGENTS.md)

本文件沉淀 `native-isp-vps` / `Native ISP Cockpit` 的关键工程约束与真实踩坑经验，供后续开发者与 AI 代理遵守。

## 1. 严禁在未签名 macOS 桌面应用中使用 Keychain (`keyring`) 或在轮询热路径读取凭据

- **背景与事故**：
  - 本地编译或未配备 Apple Developer ID 签名的 macOS Tauri App 属于 ad-hoc 签名。
  - `keyring`（`apple-native` 底层调用 `SecKeychainFindGenericPassword`）在 ad-hoc 签名应用上无法稳定持久化「始终允许」ACL；一旦用户在高频轮询（如 `poll_once` 每 2 秒一次）中无缓存地调用 `load_secrets()`，macOS 会每 2 秒弹出一次「登录钥匙串」密码授权框，形成无限弹窗死循环。
- **硬性约束**：
  1. **不要引入 `keyring` / macOS Keychain API** 存储未签名桌面应用的节点凭据；统一使用 `~/.config/native-isp-vps/secrets.json`，并在创建与写入时强制 `0600` 权限（目录 `0700`，见 `app/src-tauri/src/storage.rs`）。
  2. **所有凭据读取必须走进程内内存缓存**（`OnceLock<Mutex<HashMap<String, NodeSecrets>>>`）：仅在首次读取时加载磁盘、仅在写入/删除时同步落盘，轮询热路径（如 `poll_once`）严禁直接触发磁盘或系统凭据库 IO。
  3. 仅当未来具备稳定的 Apple Developer ID 签名与公证流水线时，才可重新评估系统 Keychain 集成，且依然必须保留内存缓存层，禁止热路径直查 Keychain。
