# 主流海外 VPS 选型速查指南

未购买服务器前，先看清 **IP 属性与回程线路** 差异，再按需直接选型。

## 一、核心类型对比

| 类型 | 本质与特点 | 适合场景 | 代表商家 |
| :--- | :--- | :--- | :--- |
| **双 ISP（俗称伪家宽）** | 机房托管，但 IP 在风控库标注为双 ISP；欺诈分极低、几乎不弹验证、自带 9929/CMIN2 回程；单机性价比最高，但基础带宽偏保守（约 30–50Mbps） | **绝大多数个人/AI 开发者首选**（一台机器兼顾纯净度与线路） | **LisaHost（丽萨主机）** |
| **一线优化线路 IDC** | 纯数据中心 IP，但拥有顶级 CN2 GIA / CMIN2 / 9929 专线；大带宽、晚高峰零抖动；面对极严风控站可能需配双 ISP 落地 | 预算充足、追求极致带宽与稳定性的重度开发 | **DMIT（大妈）**、**BandwagonHOST（搬瓦工）**、**SpartanHost（斯巴达）** |
| **廉价普通 BGP 机房** | 低价纯机房 + 普通国际线路；晚高峰易丢包、IP 滥用严重 | 仅适合练手与备用，**不建议登录重要 AI 账号** | **RackNerd**、**CloudCone**、**Oracle Cloud** |

## 二、主流商家速览

| 商家 | 官网 | 主打线路 | IP 属性 | 核心优缺点 |
| :--- | :--- | :--- | :--- | :--- |
| **LisaHost（丽萨主机）** | [lisahost.com](https://lisahost.com) | 美西 9929 / 4837 / CMIN2 | 双 ISP（伪家宽） | 欺诈分低、解锁与 AI 友好、中文支付宝；非物理家宽、基础口子约 30–50Mbps，用纯 TCP Reality Vision 可稳跑满 |
| **DMIT（大妈）** | [dmit.io](https://www.dmit.io) | 洛杉矶 CN2 GIA / EB (CMIN2) / 日港 | 纯 IDC | 线路与硬件第一梯队、晚高峰极稳；纯机房 IP、特价款常缺货 |
| **BandwagonHOST（搬瓦工）** | [bwh81.net](https://bwh81.net) | 洛杉矶 CN2 GIA-E / 日荷港 | 纯 IDC | 十几年老牌稳定、支持快照与换机房；纯机房 IP、门槛较高 |
| **SpartanHost（斯巴达）** | [spartanhost.org](https://spartanhost.org) | 西雅图 AS4837 + 高防 | 纯 IDC | 联通友好、带高防；电信/移动晚高峰弱于 GIA/CMIN2 |
| **RackNerd / CloudCone** | [racknerd.com](https://racknerd.com) | 洛杉矶普通 BGP | 廉价 IDC | 年付 \$10–\$25 极便宜；晚高峰丢包、IP 脏 |

## 三、选购 4 条硬标准

1. **独立公网 IPv4**（避开 NAT 共享 IPv4）。
2. **系统选 Debian 11 或 Debian 12（x86_64 / amd64）**。
3. **硬件建议**（1–5 人共享足够）：1 核 / 512MB–1GB 内存 / 10GB+ SSD / 1TB+ 月流量。
4. **外层安全组**：若商家自带防火墙，确保放行 `443/tcp`、SSH 端口及面板/订阅端口。

## 四、买好后发给 AI 的一键部署模板

```text
请用 native-isp-vps 帮我全自动部署这台 VPS 并生成万能订阅，顺手加固本机 Clash Verge Rev：
- 服务器 IP：<VPS_IP>
- SSH 端口：<默认 22 或自定义端口>
- 用户名：root
- 密码：<SSH_PASSWORD>
- 用户与流量：2 人（Me / GF），总流量 1TB/月，各 500GB，每月 25 号重置
```
