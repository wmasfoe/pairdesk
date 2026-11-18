# 行为对照表

> 这份文档回答：**旧实现的每个行为，对应哪个单测，新实现在哪。**

## 使用方式

迁移后的任何行为偏差，都能在这张表里定位到具体单测。改代码前先跑对应单测，
若单测红灯说明改动破坏了既有行为。

## 凭据生成 → `pairdesk-shell/src/credentials.rs`

| 旧行为 | 旧位置 | 单测 |
|---|---|---|
| 密码 6 位数字 | `HostPage.tsx::genPwd` | `密码恒为六位数字` |
| 密码范围 [100000, 999999] | 同上 | `密码落在原实现的值域内` |
| 会话码 `pd-` + 4 位 + 3 位数字 | `HostPage.tsx::genSid` | `会话码形状与原实现一致` |
| 随机性 | — | `生成的凭据具有足够随机性` |
| 复制文本格式 | `HostPage.tsx::copyShare` | `复制文本格式与原实现逐字一致` |
| 按钮启用条件 | `HostPage.tsx` 的 `disabled` | `完整性判定与按钮禁用条件一致` |

## 会话状态机 → `pairdesk-shell/src/session.rs`

| 旧行为 | 旧位置 | 单测 |
|---|---|---|
| 活跃阶段 = auth/auth'd/connected | `HostPage.tsx::active` | `活跃阶段判定与旧实现一致` |
| 脉冲动画条件 | `StatusDot pulse` prop | `脉冲动画只在等待阶段` |
| 色调映射表 | `HostPage/ViewerPage::TONE` | `色调映射与旧实现一致` |
| 被控端文案 | `HostPage::statusLabel` | `被控端文案与旧实现一致` |
| 控制端文案 | `ViewerPage::labelOf` | `控制端文案与旧实现一致` |
| 模式解析与默认值 | `ConnectionMode` 类型 | `连接模式解析与默认值` |
| 模式序列化 | `tauri.ts::serializeInput` | `连接模式序列化可被内核识别` |
| 中继地址显示条件 | `HostPage` 的 `(mode==='relay'\|\|'auto')` | `只有中继与自动模式需要中继地址` |
| 开关关闭拒绝起被控端 | `bridge.rs:108` | `开关关闭时拒绝起被控端` |
| 开关打开允许起被控端 | 同上 | `开关打开后允许起被控端` |
| 缺凭据拒绝 | `HostPage` 按钮 disabled | `缺凭据时拒绝且不改阶段` |
| 控制端不受开关约束 | — | `控制端不需要允许开关` |
| 活跃会话拒绝重复连接 | — | `活跃会话中不能重复连接` |
| 新会话清空残留 | `useSession.ts` setState | `开始新会话会清空上一轮残留` |
| 认证成功 → authenticated | `useSession.ts` authResult | `认证成功进入已认证并清除错误` |
| 认证失败 → error + reason | 同上 | `认证失败进入错误态并使用内核原因` |
| 认证失败默认文案 | `e.reason ?? '认证失败'` | `认证失败无原因时回落到默认文案` |
| 对端连接/断开 | `useSession.ts` peer* | `对端连接与断开驱动阶段` |
| 分辨率事件 | `useSession.ts` size | `分辨率事件填充屏幕尺寸` |
| 传输路径与提示 | `useSession.ts` transport/notice | `传输路径与提示分别填充` |
| 错误事件 | `useSession.ts` error | `错误事件进入错误态` |
| 帧/统计不触发状态变化 | `useSession.ts` default 分支 | `画面帧与统计不改变会话状态` |
| 仅连接后转发输入 | `bridge.rs::pd_send_input` | `仅在已连接时转发输入` |
| 断开 → disconnected | `useSession.ts::disconnect` | `断开返回停止意图并进入已断开` |
| 默认状态 | `useSession.ts` 初始 useState | `默认状态与旧实现初始值一致` |
| 默认画质 jpeg:80 fps:20 | `bridge.rs::pd_start_host` | `默认画质与旧实现硬编码一致` |
| 拒绝文案齐全 | `bridge.rs:108` | `拒绝原因文案齐全` |

## 输入映射 → `pairdesk-shell/src/input.rs`

| 旧行为 | 旧位置 | 单测 |
|---|---|---|
| 1:1 坐标恒等映射 | `VideoView::getRemoteCoord` | `坐标换算在无缩放时是恒等映射` |
| 按比例放大到远端分辨率 | 同上 | `坐标换算按比例放大到远端分辨率` |
| 考虑画面偏移 | 同上 | `坐标换算考虑画面在窗口内的偏移` |
| 越界坐标夹紧 | 同上 `Math.max/min` | `坐标换算把越界坐标夹紧到画面边界` |
| 非法矩形跳过 | `if (rect.width <= 0) return null` | `非法矩形拒绝换算` |
| 鼠标按键 0/1/2 → 1/2/3 | `VideoView::handleMouseDown` | `鼠标按键映射与旧实现一致` |
| 未知按键当左键 | 同上三元表达式末尾 | `未知鼠标按键回落到左键` |
| 修饰键位 1/2/4/8 | `VideoView::handleKeyDown` | `修饰键位掩码与内核常量一致` |
| 多修饰键叠加 | 同上 | `多修饰键可叠加` |
| 特殊键 keysym | `VideoView::keyToSym` switch | `特殊键映射为正确的_x11_keysym` |
| 方向/导航键 keysym | 同上 | `方向键与导航键映射正确` |
| 修饰键 keysym | 同上 | `修饰键本身映射为_x11_keysym` |
| 可打印 ASCII 用码点 | 同上 `charCodeAt` | `可打印_ascii_用码点作为_keysym` |
| 非 ASCII/IME 返回 0 | 同上 `return 0` | `非_ascii_与_ime_按键返回不发送` |
| 控制字符不发送 | 同上 | `控制字符不发送` |
| 无法映射不构造消息 | 同上 `if (sym === 0) return` | `按键消息在无法映射时不构造` |
| 鼠标消息自动换算 | `VideoView` 三处 | `鼠标移动消息自动完成坐标换算` |
| 转换为内核协议类型 | `tauri.ts::serializeInput` | `输入消息可转换为内核协议类型` |

## 权限引导 → `pairdesk-shell/src/permissions.rs`

| 旧行为 | 旧位置 | 单测 |
|---|---|---|
| 非 macOS 永不显示 | `PermissionBanner` 首行 return null | `非_macos_平台永不显示引导` |
| 权限齐全不显示 | 同上 | `权限齐全时不显示引导` |
| 被控端只看屏幕录制 | `requiredFor='host'` | `被控端只关心屏幕录制权限` |
| 控制端只看辅助功能 | `requiredFor='viewer'` | `控制端只关心辅助功能权限` |
| 首页两者都看，顺序固定 | `requiredFor='both'` | `首页两类权限都检查且顺序稳定` |
| 只列缺失项 | `needScreen && needAccessibility` | `部分缺失时只列出缺失项` |
| 角色权限对应关系 | 同上 | `角色与权限的对应关系` |
| 权限字符串 | `usePermissions::request` | `权限类型字符串可被内核识别` |
| 权限展示名称 | `PermissionBanner` JSX | `权限类型有展示名称` |
| 检测失败不阻塞 | `usePermissions.ts` catch | `检测失败时兜底为不阻塞` |
| 横幅标题 | `PermissionBanner` h4 | `横幅标题与旧实现一致` |

## 更新检查 → `pairdesk-shell/src/update.rs`

| 旧行为 | 旧位置 | 单测 |
|---|---|---|
| 去 v 前缀 | `bridge.rs::trim_start_matches` | `版本号去掉_v_前缀` |
| 版本相同不更新 | `bridge.rs::has_update` | `版本相同时无需更新` |
| 版本不同要更新 | 同上 | `版本不同时需要更新` |
| 空 tag 不更新 | `!tag_name.is_empty()` | `空_tag_视为无需更新` |
| 朴素比较（含降级误报） | 同上字符串比较 | `朴素比较保留原行为_含降级误报` |
| macOS 按架构匹配 dmg | `target_pattern` 分支 | `macos_按架构匹配_dmg` |
| macOS 未知架构回退 intel | 同上 else 分支 | `macos_未知架构回落到_intel` |
| Windows/Linux 匹配 | 同上 | `windows_与_linux_匹配正确` |
| 当前平台识别 | cfg! 宏 | `当前平台能被识别` |
| 挑第一个匹配资产 | `assets.find_map` | `挑选第一个匹配的资产` |
| 跳过无 url 资产 | 同上 `if let Some` | `跳过没有下载地址的资产` |
| 无匹配返回 None | 同上 | `无匹配资产时返回_none` |
| 组装有匹配资产 | `bridge.rs::UpdateInfo` | `组装更新信息_有匹配资产` |
| 无匹配回退发布页 | `unwrap_or_else(html_url)` | `组装更新信息_无匹配资产时回落到发布页` |
| 已是最新 | 同上 | `组装更新信息_已是最新` |

## GPUI 适配器层 → `crates/pairdesk-ui/`

### 画面帧（`frame.rs`）

| 行为 | 单测 |
|---|---|
| 合法 JPEG 解码尺寸正确 | `解码合法_jpeg_得到正确尺寸` |
| 输出 RGBA 四通道 | `解码输出是_rgba_四通道` |
| 非法数据不崩溃 | `解码非法数据返回错误而不崩溃` |
| 帧泵初始状态 | `帧泵初始状态为空` |
| 事件分流 | `事件分流把画面帧与其他事件分开` |
| 解码性能预算（仅 release） | `解码性能满足_20fps_预算` |

### 输入捕获（`video_view.rs`）

| 行为 | 单测 |
|---|---|
| GPUI 按键枚举 → 协议编号 | `gpui_鼠标按键映射到协议编号` |
| 修饰键四键齐全 | `修饰键映射覆盖全部四个` |
| 无修饰键时掩码为零 | `无修饰键时位掩码为零` |
| 行数滚轮折算像素 | `行数滚轮按四十像素折算` |
| 像素滚轮原样透传 | `像素滚轮原样透传` |
| 画面矩形初始为空 | `共享_bounds_初始为空` |

### 会话启动（`session_host.rs`）

| 行为 | 单测 |
|---|---|
| 地址解析（IP/主机名） | `地址解析支持_ip_与主机名` |
| 地址解析拒绝非法输入 | `地址解析拒绝非法输入` |
| 错误意图被拒绝 | `错误意图被拒绝` |

### 更新安装（`update.rs`）

| 行为 | 单测 |
|---|---|
| 安装包扩展名与平台匹配 | `安装包扩展名与平台匹配` |
| 当前版本非空 | `当前版本非空` |
| 空地址拒绝下载 | `空地址拒绝下载` |

## 统计

| 层 | 单测数 |
|---|---|
| `pairdesk-shell` | 81 |
| `pairdesk-ui` | 18 |
| `pairdesk-core`（既有） | 16 |
| **合计** | **115** |

> 说明：原 `event.rs`（内核事件 → 视图事件的换名映射，10 个单测）已移除。
> 那是 Tauri IPC 需要 JSON 序列化时代的产物，GPUI 同进程直接消费 `CoreEvent`，
> 该层属纯死代码。
