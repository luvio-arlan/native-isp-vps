# Native ISP Cockpit · GitHub Releases 自动更新配置指南

本应用内置了 `tauri-plugin-updater` + GitHub Actions 自动发布流水线（`.github/workflows/release-app.yml`）。配置一次签名密钥后，每次在 GitHub 推送新版本 Tag（如 `v0.2.0`），所有已安装用户的桌面看板都会自动检测到新版本并支持 **应用内一键下载、验签与重启更新**。

## 首次发布前的 3 步一次性配置

### 1. 生成 Tauri Ed25519 签名密钥对（仅需执行一次）

```bash
cd app
pnpm tauri signer generate -w ~/.tauri/native-isp-cockpit.key
```

命令会输出两样东西：
- **私钥文件**：`~/.tauri/native-isp-cockpit.key`（严禁提交到 Git！）
- **公钥字符串**：一段 Base64 文本

### 2. 配置公钥与 GitHub Secrets

1. 将**公钥字符串**填入 [tauri.conf.json](./src-tauri/tauri.conf.json) 的 `plugins.updater.pubkey` 字段。
2. 在 GitHub 仓库 `Settings → Secrets and variables → Actions` 新增两个 Repository Secret：
   - `TAURI_SIGNING_PRIVATE_KEY`：粘贴 `~/.tauri/native-isp-cockpit.key` 的完整内容
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`：生成密钥时设置的密码（无密码则留空字符串）

### 3. 发版触发全平台自动构建与客户端热更新

同步更新 `app/package.json`、`app/src-tauri/Cargo.toml`、`app/src-tauri/tauri.conf.json` 中的版本号后：

```bash
git add -A
git commit -m "chore(cockpit): release v0.2.0"
git tag v0.2.0
git push origin main --tags
```

GitHub Actions 会自动编译 macOS（Apple Silicon + Intel）和 Windows 安装包，并生成带签名的 `latest.json` 上传至 GitHub Releases；客户端启动或点击「检查更新」时会自动拉取 `latest.json` 完成升级。
