---
name: native-isp-vps
description: >-
  面向 AI 开发者的海外 VPS 全自动部署与网络稳定调优 Agent Skill。
  闭环覆盖「需求引导 → VPS 选型 → SSH 内核调优（FQ+BBR+1GB 友好缓冲+禁 TFO/ECN+120s 保活）→ 3x-ui 非交互部署 → 纯 TCP VLESS+Reality+Vision 与 Xray sockopt/DNS 缓存加固 → 全平台防泄露/禁 QUIC 万能订阅 → 本机 Clash Verge Rev 加固与端到端验真」。
  在以下场景主动激活：自建独享海外节点、优化现有 VPS 断流/延迟、稳定使用海外 AI 工具，或一键生成全平台防泄露订阅与本机 Clash Verge Rev 加固时。
---

# Native ISP VPS · 海外 VPS 全自动部署与调优

> **核心方案**：纯 TCP `VLESS + Reality + Vision (443/tcp)` + 跨洋 TCP/BBR/FQ 内核调优 + Xray `sockopt`/DNS 缓存 + 禁代理内 QUIC UDP/443 + 全平台防泄露万能订阅 + 本机 Clash Verge Rev 并发保活加固。零手工踩坑，全天候高稳不断流。

---

## Stage 0 · 交互式需求引导（未提供完整信息时先问清）

一次性向用户确认以下信息：

1. **是否已购买 VPS？**
   - **未购买**：读取并分享 [vps-guide.md](./references/vps-guide.md)，建议直接选择 **单台靠谱的美西原生双 ISP（9929 / CMIN2 / CN2 GIA）** + **Debian 11 / 12 (x86_64)** + 独立公网 IPv4（避开 384MB–512MB 廉价香港/亚太小机，晚高峰极易抖动丢包且徒增双机分流复杂度），买好后提供登录凭证即可。
   - **已购买**：收集（能自动读取的优先自动读取，未知项向用户确认）：
     - **SSH 凭证**：IP、端口（默认 22）、用户名（通常 `root`）、密码或私钥路径
     - **月度流量与重置日**：VPS 月总流量（如 `1TB` / `2TB`）与账单重置日（如每月 `25` 号），用于 Clash 卡片与看板展示剩余流量进度条；**默认单机单订阅全员共享全部流量（不限设备数、最纯净省心），除非用户明确要求拆分为多用户独立配额**
     - **本机加固与看板**：是否顺手自动优化本机 Clash Verge Rev（禁 IPv6、Fake-IP DoH、TUN `Mixed` + `Merge` 并发保活），以及是否一键安装并自动接入本地 **Native ISP Cockpit** 领航看板（支持窗口化驾驶舱 + 状态栏实时网速/延迟 + GitHub 自动更新）

> [!IMPORTANT]
> **安全边界**：SSH 密码、3x-ui 账密、UUID 与 Sub ID 严禁写入 Git 仓库或公开文档，交付后提醒用户存入密码管理器。

---

## Stage 1 · 系统验真、内核调优、冗余 DNS 与 Swap

通过 SSH（优先密钥或 `SSH_ASKPASS`/`expect` 免交互）登录后一次执行：

1. **验真系统、硬件、IP 归属与带宽**：
   ```bash
   cat /etc/os-release | grep PRETTY_NAME
   uname -a && free -h && nproc
   curl -s https://ipinfo.io
   curl -s -o /dev/null -w "CF 25MB down: %{speed_download} B/s (time=%{time_total}s)\n" "https://speed.cloudflare.com/__down?bytes=25000000"
   ```
   记录 `country`、`city`、`org` 用于 Stage 3 节点命名。
2. **写入跨洋 TCP 内核调优**：
   - 将 [sysctl-bbr-tuning.conf](./references/sysctl-bbr-tuning.conf) 写入 `/etc/sysctl.d/99-vps-tuning.conf` 并执行 `sysctl --system`。
   - **必须热切并持久化主网卡队列为 `fq`**（否则未重启前 BBR 无法精准发包，且部分云镜像重启后网卡早于 sysctl 初始化会退回默认队列）：
     ```bash
     IFACE=$(ip route show default | awk '/default/ {print $5; exit}')
     tc qdisc replace dev "$IFACE" root fq
     cat >/etc/systemd/system/vps-fq-qdisc.service <<'EOF'
     [Unit]
     Description=Ensure FQ qdisc for BBR on default interface
     After=network-online.target
     Wants=network-online.target

     [Service]
     Type=oneshot
     ExecStart=/bin/sh -c 'IFACE=$(ip route show default | awk "/default/ {print \$5; exit}"); [ -n "$IFACE" ] && /sbin/tc qdisc replace dev "$IFACE" root fq'
     RemainAfterExit=yes

     [Install]
     WantedBy=multi-user.target
     EOF
     systemctl daemon-reload && systemctl enable --now vps-fq-qdisc.service
     tc qdisc show
     ```
   - 关键参数说明：1GB 级小内存保持 `256KB` 默认 / `16MB` 上限缓冲（2GB+ 千兆口可升至 `32MB`）；`tcp_fastopen=0` & `tcp_ecn=0` 避免国内运营商 SYN 黑洞；`tcp_keepalive_time=120` 防止 NAT 静默断连；`tcp_mtu_probing=1` + `tcp_slow_start_after_idle=0` + `tcp_notsent_lowat=16384` 降低交互延迟。
3. **探测并加固 `/etc/resolv.conf` 冗余 DNS（只写入实测可达的上游）**：
   > [!WARNING]
   > 部分机房会屏蔽个别公共 DNS（实测有机房 `1.0.0.1` 100% 不可达）。Xray REALITY 对**每条新连接**都会解析一次伪装目标，若 `resolv.conf` 掺入不可达上游并启用 `rotate`，约 1/3 的解析会先等满超时，表现为「新开网页/AI 请求随机慢 2 秒」。因此必须先探测、只保留可达者、按优先级顺序写入且**不使用 `rotate`**。
   ```bash
   command -v dig >/dev/null || apt-get install -y -qq dnsutils >/dev/null 2>&1
   GOOD_DNS=""
   for ns in 1.1.1.1 8.8.8.8 1.0.0.1 8.8.4.4 9.9.9.9; do
     ok=0; for i in 1 2 3; do dig +time=1 +tries=1 @"$ns" www.cisco.com A >/dev/null 2>&1 && ok=$((ok+1)); done
     [ "$ok" -eq 3 ] && GOOD_DNS="$GOOD_DNS $ns"
   done
   echo "reachable upstreams:$GOOD_DNS"
   if [ -n "$GOOD_DNS" ]; then
     # 若为 resolvconf / systemd-resolved 管理的软链接，先解除，避免 DHCP 续约覆盖
     [ -L /etc/resolv.conf ] && rm -f /etc/resolv.conf
     { for ns in $(echo $GOOD_DNS | cut -d' ' -f1-3); do echo "nameserver $ns"; done
       echo "options timeout:1 attempts:2"; } > /etc/resolv.conf
   fi
   for i in 1 2 3 4 5 6; do s=$(date +%s%N); getent ahostsv4 www.cisco.com >/dev/null; echo "$(( ($(date +%s%N) - s) / 1000000 ))ms"; done  # 每次都应 < 100ms
   ```
   记录 `GOOD_DNS` 中前两个上游，Stage 3 的 Xray 内置 DNS 复用它们。
4. **小内存自动加 1GB Swap**：检查 `swapon --show`，若无 Swap 且磁盘充足，创建 1GB `/swapfile`（`chmod 600`、`mkswap`、`swapon` 并写入 `/etc/fstab`）。
5. **检查端口**：确认 `443/tcp` 未被占用。

---

## Stage 2 · 非交互安装 3x-ui 面板

1. 安装依赖：`apt-get update && apt-get install -y curl wget sqlite3 jq tar tzdata openssl ca-certificates`
2. 非交互安装最新官方 3x-ui：
   ```bash
   curl -Ls https://raw.githubusercontent.com/mhsanaei/3x-ui/master/install.sh -o /tmp/3x-ui-install.sh
   export DEBIAN_FRONTEND=noninteractive XUI_NONINTERACTIVE=1 XUI_SSL_MODE=none XUI_DB_TYPE=sqlite XUI_SERVER_IP="<VPS_IP>" XUI_ENABLE_FAIL2BAN=false
   bash /tmp/3x-ui-install.sh
   ```
3. 从输出或 `/etc/x-ui/install-result.env` 提取 `Username`、`Password`、`Port`、`WebBasePath`、`Access URL`、`API Token`。
4. 确认 `systemctl is-active x-ui` 为 `active`，面板端口与订阅端口（默认 `2096`）已监听。

---

## Stage 3 · 部署纯 TCP VLESS + Reality + Vision 与安全加固

统一使用 **纯 TCP `VLESS + Reality + Vision (443/tcp)`**——每条连接独立 TCP 互不连坐、零白天 UDP QoS 断流、全端免脚本开箱即稳。

1. **自动优选低延迟 Reality 伪装目标**（避开 `www.apple.com`，强制 `-4` 防纯 IPv4 机浪费 2s 回退）：
   - 实测 `www.cisco.com`、`www.oracle.com`、`www.shopify.com`、`www.microsoft.com`、`aws.amazon.com`、`www.datadoghq.com`：
     ```bash
     curl -4 -sI --http2 --tlsv1.3 --max-time 3 -w "code=%{http_code} ver=%{http_version} t_conn=%{time_connect} t_tls=%{time_appconnect}\n" -o /dev/null "https://$d"
     ```
   - 选择 `HTTP 200`、`HTTP/2`、`TLS 1.3` 且 `t_conn` 最低的域名作为 Reality `target` / `serverNames`。
2. **静态极简命名**：
   - 基于 `ipinfo.io` 生成 `"<国旗Emoji> <City> · Dual ISP"`（如 `🇺🇸 Los Angeles · Dual ISP`，非双 ISP 用 `Native` 或 `VPS`），写入入站 `remark` 与 `settings.subTitle`。
   - `settings.remarkTemplate` 固定为 **`{{INBOUND}} · Reality`**（严禁写入动态流量变量，以免破坏 Clash `store-selected` 记忆；剩余流量由 `Subscription-Userinfo` 进度条展示）。
   - *写入 SQLite 时使用 Python ASCII Unicode 转义防止终端 Emoji 乱码。*
3. **生成密钥与共享凭证（默认单订阅，按需才拆分）**：
   - 调用 `GET http://127.0.0.1:<PANEL_PORT>/<WEB_BASE_PATH>/panel/api/server/getNewX25519Cert`（Header: `Authorization: Bearer <API_TOKEN>`）生成 Reality `privateKey` / `publicKey`。
   - **默认创建 1 个统一共享客户端**（`email: "Shared"`，生成 1 组 `UUID` `/proc/sys/kernel/random/uuid` 与 16 位 `Sub ID` `openssl rand -hex 8`，多设备/多人共用同一条万能订阅）；**仅当用户明确要求拆分多成员额度时**，再按人生成独立 `UUID`/`Sub ID`。生成 2 个 `Short IDs`（`openssl rand -hex 4`、`openssl rand -hex 8`）。
4. **创建入站并写入生产级 Xray 配置**：
   - 入站参数：`vless` · `443/tcp` · `reality` · uTLS `chrome` · `spiderX: "/"`；客户端 `enable: true`、`flow: "xtls-rprx-vision"`、`limitIp: 0`（不限同时在线设备数）。
   - **入站 `streamSettings.sockopt`**（20s 淘汰僵尸 Socket 触发秒重连）：
     ```json
     "sockopt": {
       "tcpFastOpen": false,
       "tcpNoDelay": true,
       "tcpKeepAliveIdle": 60,
       "tcpKeepAliveInterval": 15,
       "tcpUserTimeout": 20000,
       "tcpcongestion": "bbr"
     }
     ```
   - **流量配额与自动重置**：按用户提供的月总流量或面板已有配置以字节写入 `totalGB`（如 `2000GB = 2147483648000`），同步更新 `inbounds`、`clients`（`total_gb`, `traffic_reset='monthly'`, `traffic_reset_day`）与 `client_traffics`（`total`, `reset_day`）。
   - **强化 `settings.xrayTemplateConfig`**：
     1. 内置缓存 DNS：`"dns": {"servers": ["1.1.1.1", "8.8.8.8"], "queryStrategy": "UseIPv4", "disableCache": false}`（`servers` 必须替换为 Stage 1 实测可达的 `GOOD_DNS` 前两个上游）
     2. 长连接 Policy：`policy.levels.0` 写入 `"handshake": 4, "connIdle": 600, "uplinkOnly": 2, "downlinkOnly": 5`（保留 `statsUser*`）
     3. 路由防 BT：`{"type":"field","protocol":["bittorrent"],"outboundTag":"blocked"}`
     4. 纯 IPv4 出站保活：无 IPv6 出口的 VPS 在 `direct`（`freedom`）出站的 `streamSettings.sockopt` 中配置 `"domainStrategy": "UseIPv4"`、`tcpNoDelay: true`、`tcpKeepAliveIdle: 60`、`tcpKeepAliveInterval: 15`（切勿写在已废弃的 `settings.domainStrategy` 中，确保 Xray 26+ 零警告）
5. **最小暴露安全加固（UFW + Fail2ban）**：
   - 安装 `ufw` 与 `fail2ban`，仅放行 SSH 端口、`443/tcp`、订阅端口（`2096/tcp`）、面板端口（`<PANEL_PORT>/tcp`），再执行 `ufw --force enable && systemctl enable ufw`。
   - `/etc/fail2ban/jail.local` 守护实际 SSH 端口，显式配置 `backend = systemd`、`bantime = 1h`、`findtime = 10m`、`maxretry = 5`。
   - （可选进阶 · 面板零公网暴露）看板在配置 SSH 体检私钥后会自动经 `ssh -L` 加密隧道访问面板 API，因此面板端口可以不对公网放行：执行 `ufw delete allow <PANEL_PORT>/tcp`，浏览器管理时先 `ssh -N -L 2053:127.0.0.1:<PANEL_PORT> root@<VPS_IP>`，再打开 `http://127.0.0.1:2053/<WEB_BASE_PATH>`。仅在用户同意改变浏览器访问方式时执行。
6. **服务端核验**：
   - `/usr/local/x-ui/bin/xray-linux-amd64 run -test -c /usr/local/x-ui/bin/config.json` → `Configuration OK`
   - 确认 `tc qdisc show` 为 `fq`、`ss -tulpn` 监听 `443/tcp`、`ufw status` 与 `fail2ban-client status sshd` 正常。

---

## Stage 4 · 配置全平台万能订阅与防泄露分流

利用 3x-ui v3.11+ 原生 Clash/Mihomo 订阅（无需 subconverter）：

1. **写入 SQLite `/etc/x-ui/x-ui.db` 的 `settings` 表**：
   - `subEnable = 'true'`，`subPort = '2096'`
   - `subClashEnable = 'true'`，`subClashPath = '/mihomo/'`
   - `subClashEnableRouting = 'true'`，`subClashAutoDetect = 'true'`，`subClashRemoteDNS = 'true'`
   - `subTitle`：提前写入静态标题（确保首次导入自动命名卡片）
   - `remarkTemplate = '{{INBOUND}} · Reality'`
   - `subClashRules`：顶部写入 `IP-CIDR,<VPS_IP>/32,DIRECT,no-resolve`，紧接 [mihomo-rules.yaml](./references/mihomo-rules.yaml) 全部规则（禁代理内 QUIC UDP/443 + 全系海外 AI 强制代理 + Google Play CDN + 国内直连且 **所有 `GEOIP` 强制带 `,no-resolve`** 保住 Fake-IP `<1ms` 秒发；`MATCH,PROXY` 由面板兜底）。
   - **单机纯净原则**：默认一机一链直连，不主动引入多机分流（若用户确有备机，AI 服务严禁 `load-balance` 或激进 `url-test`，仅可用高粘性 `fallback`）。
2. **公网地址输出避坑**：不要设置 `subDomain`（否则本机 `127.0.0.1` 验真报 `403`）；在 `inbounds` 表设置 `share_addr_strategy = 'custom'`、`share_addr = '<VPS_IP>'`。
3. **清理临时文件并重启**：
   ```bash
   rm -f /tmp/3x-ui-install.sh /tmp/inbound.json /tmp/cookies.txt
   systemctl restart x-ui
   ```
4. **双端验真**：
   - VPS 本机：`curl -s http://127.0.0.1:2096/mihomo/<SubID>` 确认 YAML 合法、节点名静态、含禁 QUIC 规则、`server` 为公网 IP。
   - 用户本机外网：
     - `curl -sI -A "clash-verge/v2.5.10" http://<VPS_IP>:2096/<subPath>/<SubID>` → `200 OK`、`application/yaml`、含 `Profile-Title` 与 `Subscription-Userinfo`
     - `curl -sI -A "Shadowrocket/2400" http://<VPS_IP>:2096/<subPath>/<SubID>` → `200 OK`、`text/plain`（Base64 VLESS）
     - `nc -vz -G 5 <VPS_IP> 443` 确认 `443/tcp` 可达。

---

## Stage 5 · 交付、本机 Clash Verge Rev 加固、看板自动接入与速查排障

1. **清晰交付**：
   - **全平台万能订阅（首选）**：`http://<VPS_IP>:2096/<subPath>/<SubID>`（自动识别 Clash Verge Rev / FlClash / Stash 下发完整 YAML；Shadowrocket / v2rayNG 下发 Base64 VLESS）
   - **强制 Clash YAML 备用链**：`http://<VPS_IP>:2096/mihomo/<SubID>`
   - **移动端要点**：iPhone 小火箭路由选「配置」并关 IPv6；安卓 FlClash 开 Fake-IP 覆写并关 IPv6；支持多设备同时在线。
   - 附 3x-ui 管理后台地址、账密及核验摘要，提醒妥善保存。
2. **本机 Clash Verge Rev 自动加固（macOS 默认路径 `~/Library/Application Support/io.github.clash-verge-rev.clash-verge-rev/`）**：
   - 在**单条脚本内**完成：优雅退出 → 备份配置 → 修改 → 刷新 DNS 缓存 → 重新启动（防止中途断网失联）：
     1. 锁定 `ipv6: false`、`tun.strict-route: true`、`tun.stack: mixed`（切勿使用 `mips`）、`dns-hijack: [any:53, tcp://any:53]`；
     2. 将 [clash-verge-merge.yaml](./references/clash-verge-merge.yaml) 写入当前订阅绑定的 `profiles/<merge_uid>.yaml`（永久开启 `tcp-concurrent`、15s 保活与 `sniffer`）；
     3. 在 `verge.yaml` 开启 `enable_dns_settings: true`，按 [clash-verge-dns.yaml](./references/clash-verge-dns.yaml) 写入 `dns_config.yaml`；
     4. 执行 `dscacheutil -flushcache` 并 `open -a "Clash Verge"`。
   - **端到端验真**：经 Mihomo Unix Socket（`/var/run/clash-verge-service/users/<UID>/verge-mihomo.sock`）核验配置生效；验证 `curl -4 https://ipinfo.io/ip` 为 VPS IP、`curl -6 --max-time 3 https://ipinfo.io/ip` 无 IPv6 泄露、`https://www.baidu.com` 直连 `200 OK`、`https://api.anthropic.com` 稳定连通。
   - **日常最优开关**：TUN ✅开（`Mixed`）、系统代理 ❌关、DNS 覆写 ✅开、IPv6 ❌关、统一延迟 ✅开、模式 **规则（Rule）**。
3. **Stage 5.5 · 本地领航看板一键安装、免密体检与自动纳管（Zero-Touch Handoff）**：
   - **① 自动检测并安装看板 App**（macOS `/Applications/Native ISP Cockpit.app`）：
     - 若尚未安装：优先从 GitHub Releases（`https://github.com/luvio-arlan/native-isp-vps/releases/latest`）拉取最新安装包，或直接在本仓库 `app/` 目录一键本地打包安装：
       ```bash
       cd app && pnpm install && pnpm tauri build --bundles app
       rm -rf "/Applications/Native ISP Cockpit.app"
       cp -R "src-tauri/target/release/bundle/macos/Native ISP Cockpit.app" "/Applications/Native ISP Cockpit.app"
       xattr -cr "/Applications/Native ISP Cockpit.app"
       ```
     - 安装后看板内置 `tauri-plugin-updater`，后续仓库推送新版本 GitHub Release Tag 时自动检测并支持应用内一键验签更新。
   - **② 写入专用 SSH 体检公钥（让看板「内核深度体检」一键免密可用）**：
     - 在本机生成独立私钥 `~/.ssh/id_ed25519_native_isp`（若不存在），并将对应 `.pub` 追加到 VPS `/root/.ssh/authorized_keys`（`chmod 600`）。
   - **③ 执行 Zero-Touch 安全交接并拉起看板**：
     - 调用 [cockpit-handoff.sh](./scripts/cockpit-handoff.sh) 将节点交接清单以 `chmod 600` 写入 `~/.local/share/native-isp-vps/inbox/`：
       ```bash
       bash scripts/cockpit-handoff.sh <<'EOF'
       {
         "name": "🇺🇸 <CITY> · Dual ISP",
         "flag": "🇺🇸",
         "city": "<CITY>",
         "ispTag": "Dual ISP",
         "host": "<VPS_IP>",
         "vlessPort": 443,
         "panelUrl": "http://<VPS_IP>:<PANEL_PORT>/<WEB_BASE_PATH>",
         "apiToken": "<API_TOKEN>",
         "panelUsername": "<PANEL_USER>",
         "panelPassword": "<PANEL_PASS>",
         "subUrl": "http://<VPS_IP>:2096/<subPath>/<SubID>",
         "mihomoSubUrl": "http://<VPS_IP>:2096/mihomo/<SubID>",
         "realityTarget": "<REALITY_SNI>",
         "monthlyTotalGb": 2000,
         "resetDay": 25,
         "sshPort": <SSH_PORT>,
         "sshUser": "root",
         "sshKeyPath": "$HOME/.ssh/id_ed25519_native_isp",
         "source": "ai-handoff"
       }
       EOF
       open "/Applications/Native ISP Cockpit.app"
       ```
     - 看板会在 1 秒内捕获该文件、将 `apiToken`/`panelPassword` 以 `0600` 权限原子保存至本地 `~/.config/native-isp-vps/secrets.json` 并**覆零粉碎临时文件**，随后立即点亮状态栏实时网速、延迟与月度配额监控（原色钛合金 / 曜石黑双皮肤可一键切换）。
   - **④ 反向自愈闭环**：当用户从看板「内核深度体检」页复制「脱敏 AI 修复诊断包」发来时，直接按其中标出的漂移项（如 `qdisc != fq`、`DNS 上游不可达：1.0.0.1`、`ipv6Leak=true`、`Swap 未配置`、`面板 API 明文 HTTP`）执行 Stage 1 / Stage 3 / Stage 5 / Stage 5.5 定点修复并复验。
4. **3 步断流/超时速查**：
   1. 查客户端：确认 `tun.stack` 为 `mixed`、存在 `AND,((NETWORK,UDP),(DST-PORT,443)),REJECT` 规则、`Merge` 的 `tcp-concurrent` 与 `keep-alive-idle: 15` 已生效；
   2. 查内核：确认 `tc qdisc show` 为 `fq`、`tcp_fastopen=0`、`tcp_ecn=0`、`tcp_keepalive_time=120`；
   3. 查 DNS、Xray 与资源：`resolv.conf` 中每个上游都可达且无 `rotate`（连续 6 次 `getent ahostsv4 <REALITY_SNI>` 均 < 0.1s；任何一次 ≈1–2s 即为坏上游）、Xray 内置缓存 DNS 只用可达上游、`free -h` 内存与 Swap 健康。
