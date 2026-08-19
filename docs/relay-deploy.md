# PairDesk Relay 部署（VPS 一键脚本）

`relay`（中继服务器）是 PairDesk 异地互连的兜底通道：双方主动连到 relay，
relay 逐字节透明转发加密数据（**只看密文，无法解密内容**）。
它不打包进客户端，独立部署在一台公网 VPS 上即可。

## 一键部署

```bash
# 在 VPS 上（需 root 或 sudo）
sudo bash scripts/deploy-relay.sh            # 默认端口 8977
sudo bash scripts/deploy-relay.sh 9999       # 自定义端口
```

脚本做的事：

1. 从 GitHub Releases 拉取最新 `pairdesk-relay` 二进制（自动识别 x86_64 / aarch64）
2. 安装到 `/usr/local/bin/pairdesk-relay`
3. 注册 systemd 模板服务 `pairdesk-relay@<端口>.service`
   （开机自启 + `Restart=always` 崩溃自动拉起）
4. 验证端口监听并打印公网地址

## 验证

```bash
systemctl status pairdesk-relay@8977        # 服务状态
journalctl -u pairdesk-relay@8977 -f        # 实时日志
ss -ltn | grep 8977                          # 端口监听
```

## 客户端怎么填

被控端与控制端的「中继/VPS 地址」都填：

```
<公网IP>:<端口>       例如 35.212.183.245:8977
```

两端填同一个 relay 地址 + 同一个会话码（`--sid`），即可经 relay 建立会话。

## 防火墙

```bash
# ufw
sudo ufw allow 8977/tcp
# firewalld
sudo firewall-cmd --permanent --add-port=8977/tcp && sudo firewall-cmd --reload
```

## 安全说明

- relay 只做**透明字节转发 + 信令牵线**，端到端加密（ChaCha20-Poly1305）在客户端，
  relay 拿不到会话内容，也无法伪造会话。
- 会话码 + 密码双重校验由客户端完成，relay 不落盘任何密码。
- 若 VPS 带宽有限：relay 是兜底路径，正常打洞成功时流量不走 relay。

## 维护

```bash
# 重启 / 停止 / 查看
systemctl restart pairdesk-relay@8977
systemctl stop pairdesk-relay@8977
systemctl status pairdesk-relay@8977

# 卸载
systemctl disable --now pairdesk-relay@8977
rm /usr/local/bin/pairdesk-relay
```

## 升级

重新跑一次脚本即可覆盖二进制（`install` 会替换旧版本），systemd 服务自动使用新文件
（重启服务生效）。
