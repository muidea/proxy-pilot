# Xray 加载与使用 Geo 规则文件完整指南

Xray-core 仅支持基于 Protobuf 序列化的 **`.dat` 格式**规则文件（`geoip.dat` 与 `geosite.dat`），不支持直接加载 `sing-box` 专用的 `.db` 或 `.srs` 格式。切勿将 `.db` 文件直接重命名为 `.dat`，否则会导致 Xray 启动时报 Protobuf 解析失败。

---

## 一、 获取标准 `.dat` 规则文件

根据分流精细度需求，从以下任一渠道下载最新的 `geoip.dat` 和 `geosite.dat`：

- **社区增强版（推荐）：** 前往 `Loyalsoldier/v2ray-rules-dat` 仓库的 Releases 页面下载，针对大陆域名分流、防污染及广告拦截标签做了持续维护。
- **官方标准版：** 前往 `XTLS/Xray-core` 官方 Releases 发布包中获取默认内置版本。

---

## 二、 部署规则目录与路径指定

Xray 启动时会按固定优先级检索资源文件，可任选以下一种方式部署：

### 1. 放置于默认检索目录（推荐单实例使用）

- **Windows：** 直接将 `geoip.dat` 和 `geosite.dat` 放在与 `xray.exe` 同级的目录下（如在 v2rayN 中通常对应 `bin\xray\` 目录）。
- **Linux：** 默认依次检索 `/usr/local/share/xray/`、`/usr/share/xray/` 以及 `xray` 二进制可执行文件所在目录。

### 2. 通过环境变量指定自定义目录（适合多实例共享）

通过设置 `XRAY_LOCATION_ASSET` 环境变量统一指定规则文件存放路径：

**Windows PowerShell：**

```powershell
# 当前终端临时生效
$env:XRAY_LOCATION_ASSET = "D:\Proxy\assets"
.\xray.exe run -c config.json

# 写入当前用户环境变量（永久生效）
[Environment]::SetEnvironmentVariable("XRAY_LOCATION_ASSET", "D:\Proxy\assets", "User")
```

**Linux（systemd 服务配置）：**

编辑 `xray.service`，在 `[Service]` 段添加环境变量声明：

```ini
[Service]
Environment="XRAY_LOCATION_ASSET=/opt/xray/rules"
```

保存后重载并重启服务：

```bash
sudo systemctl daemon-reload && sudo systemctl restart xray
```

---

## 三、 在 `config.json` 中编写调用规则

文件部署完成后，无需在 JSON 配置中填写磁盘路径，直接在 `routing` 或 `dns` 模块中按前缀语法引用。

### 1. 前置关键配置开关

- **设置路由域名解析策略：** 在 `routing` 中将 `"domainStrategy"` 设为 `"IPIfNonMatch"`（或 `"IPOnDemand"`）。若保持默认的 `"AsIs"`，未命中域名规则的纯域名请求不会解析为 IP，会导致后续 `geoip:cn` 等 IP 规则无法命中。
- **启用流量嗅探：** 在 `inbounds` 中开启 `"sniffing"` 并设置 `"routeOnly": true`，使 Xray 能够从纯 IP 流量中提取 TLS SNI 或 HTTP Host 还原域名，确保 `geosite` 规则正常生效且不破坏原始连接目标。

### 2. 规则引用语法

- **内置标准文件调用：**
  - 常用域名集：`geosite:cn`（大陆域名）、`geosite:geolocation-!cn`（非大陆常见域名）、`geosite:category-ads-all`（广告域名拦截）、`geosite:private`（私有局域网域名）。
  - 常用 IP 集：`geoip:cn`（大陆 IP 段）、`geoip:private`（私有保留网段，如 `127.0.0.0/8`、`10.0.0.0/8`、`192.168.0.0/16`）。
- **第三方自定义 `.dat` 文件调用（`ext:` 语法）：**
  - 若资源目录下存放了自定义命名的 `.dat` 文件（如 `custom_site.dat`），可通过 `ext:文件名:标签名` 格式调用。

### 3. 路由配置示例

```json
{
  "inbounds": [
    {
      "tag": "socks",
      "port": 10808,
      "listen": "127.0.0.1",
      "protocol": "socks",
      "sniffing": {
        "enabled": true,
        "destOverride": ["http", "tls", "quic"],
        "routeOnly": true
      },
      "settings": {
        "auth": "noauth",
        "udp": true
      }
    }
  ],
  "routing": {
    "domainStrategy": "IPIfNonMatch",
    "rules": [
      {
        "type": "field",
        "domain": ["geosite:category-ads-all"],
        "outboundTag": "block"
      },
      {
        "type": "field",
        "domain": ["ext:custom_site.dat:my_proxy_list"],
        "outboundTag": "proxy"
      },
      {
        "type": "field",
        "ip": ["geoip:private"],
        "outboundTag": "direct"
      },
      {
        "type": "field",
        "domain": ["geosite:cn"],
        "outboundTag": "direct"
      },
      {
        "type": "field",
        "ip": ["geoip:cn"],
        "outboundTag": "direct"
      },
      {
        "type": "field",
        "network": "tcp,udp",
        "outboundTag": "proxy"
      }
    ]
  }
}
```

---

## 四、 配置校验、排错与自动更新

### 1. 启动前配置预检

执行以下命令验证 `.dat` 文件路径是否正确以及引用的分类标签是否存在：

```bash
xray run -test -c config.json
```

终端输出 `Configuration OK.` 即代表规则文件加载及语法校验通过。

### 2. 常见报错排查

- **`failed to load geoip.dat / geosite.dat: open ... The system cannot find the file specified`**
  - 原因：运行目录或默认检索路径下缺少 `.dat` 文件，或 `XRAY_LOCATION_ASSET` 路径指向错误。
- **`country code XXX not found in geosite.dat`**
  - 原因：当前使用的精简版 `geosite.dat` 未包含该分类标签，替换为社区完整版 `geosite.dat` 即可。
- **启动时报 Protobuf 解码失败**
  - 原因：文件下载不完整（如 0 字节文件），或误将 `sing-box` 的 `.db` 文件改名成了 `.dat`。

### 3. 定期更新维护

- **GUI 客户端（如 v2rayN）：** 通过客户端顶部菜单 **检查更新 -> 更新 Geo 文件** 自动拉取覆盖。
- **Linux 服务端/网关定时更新示例：** 建议先下载至临时文件并校验文件非空后再覆盖原文件，避免网络异常产生空文件导致服务重启失败：

```bash
#!/usr/bin/env bash
set -euo pipefail

ASSET_DIR="/usr/local/share/xray"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

curl -fsSL -o "$TMP_DIR/geoip.dat" "[https://github.com/Loyalsoldier/v2ray-rules-dat/releases/latest/download/geoip.dat](https://github.com/Loyalsoldier/v2ray-rules-dat/releases/latest/download/geoip.dat)"
curl -fsSL -o "$TMP_DIR/geosite.dat" "[https://github.com/Loyalsoldier/v2ray-rules-dat/releases/latest/download/geosite.dat](https://github.com/Loyalsoldier/v2ray-rules-dat/releases/latest/download/geosite.dat)"

if [[ -s "$TMP_DIR/geoip.dat" && -s "$TMP_DIR/geosite.dat" ]]; then
  install -m 644 "$TMP_DIR/geoip.dat" "$ASSET_DIR/geoip.dat"
  install -m 644 "$TMP_DIR/geosite.dat" "$ASSET_DIR/geosite.dat"
  systemctl restart xray
fi
```