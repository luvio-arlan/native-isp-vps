# Native ISP VPS · 海外 VPS 全自动部署与调优 Agent Skill

> **一个极简、开箱即用的 AI Agent Skill：把海外 VPS 选型、跨洋 TCP 内核调优、3x-ui + 纯 TCP VLESS Reality Vision 部署、全平台防泄露万能订阅与本机 Clash Verge Rev 加固，全部封装成全自动工作流。**

---

## 核心特性

- **极稳单一最优协议**：专注纯 TCP **VLESS + Reality + Vision (`443/tcp`)**，每条流独立 TCP 互不连坐、零白天 UDP QoS 断流、全端免脚本开箱即稳。
- **跨洋 TCP 内核 + Xray 双层调优**：热切网卡 `fq` + `BBR`、1GB 小内存友好缓冲（`256KB` 默认 / `16MB` 上限）、显式关闭 `tcp_fastopen`/`tcp_ecn` 消除 SYN 黑洞、`120s` 内核保活 + Xray `sockopt`（`tcpUserTimeout=20000` 秒级淘汰死连接）+ 内置双上游 IPv4 缓存 DNS。
- **全平台万能一条链 + 防泄露分流**：开启 3x-ui 原生 UA 识别，一条链接通吃 Clash Verge Rev / FlClash / Stash（YAML）与 Shadowrocket / v2rayNG（Base64）；内置 **禁代理内 QUIC UDP/443 提速**、全系海外 AI 域名强制代理、国内直连及原生剩余流量进度条。
- **最小安全暴露 + 本机一键加固**：服务端 UFW 最小端口白名单 + Fail2ban `systemd` 防爆破 + 防 BT；本机自动加固 Clash Verge Rev（`Mixed` TUN + 禁 IPv6 + Fake-IP DoH + `Merge` 并发保活嗅探）。

---

## 快速上手（3 步）

1. **安装 Skill**：
   ```bash
   git clone https://github.com/luvio-arlan/native-isp-vps.git
   ```
   放入对应 AI 工具的 Skills 目录：
   - **Antigravity**：`~/.gemini/config/skills/native-isp-vps/`
   - **Claude Code**：`~/.claude/skills/native-isp-vps/`
   - **Cursor**：`<project>/.cursor/skills/native-isp-vps/`

2. **告诉 AI 你的需求**：
   > 「用 `native-isp-vps` 帮我部署一台海外 VPS 节点（或帮我选一台 VPS）。」

3. **导入万能订阅**：AI 全自动完成 SSH 部署、调优与端到端验真后，复制返回的万能订阅链接导入客户端即可使用。

---

## 仓库结构

```text
native-isp-vps/
├── SKILL.md                      # AI Agent 技能主文件（5 阶段全自动部署与加固 SOP）
├── README.md                     # 项目说明与快速上手
├── LICENSE                       # MIT License
└── references/
    ├── vps-guide.md              # 主流海外 VPS 客观选型速查
    ├── sysctl-bbr-tuning.conf    # 1GB 友好跨洋 TCP 内核调优（FQ + BBR + 抗断流）
    ├── mihomo-rules.yaml         # 禁 QUIC UDP/443 + 全系海外 AI 防泄露 + 国内直连规则
    ├── clash-verge-dns.yaml      # Clash Verge Rev 防 IPv6/DNS 污染 Fake-IP 配置
    └── clash-verge-merge.yaml    # Clash Verge Rev 订阅 Merge 配置（并发建连 + 15s 保活 + 嗅探）
```

---

## 免责声明

本项目仅提供标准 Linux 运维自动化与网络调优参考，请遵守当地法律法规及服务商条款。切勿将个人服务器 IP、SSH 密码、面板账密或订阅链接提交至任何公开仓库。
