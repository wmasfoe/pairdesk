# 重构规格：Tauri → GPUI

> 这份文档回答：**重构的目标是什么，什么算做完了，哪些行为必须一模一样。**

## 1. 目标

| 项 | 现状（Tauri） | 目标（GPUI） |
|---|---|---|
| UI 技术 | WebView + React 19 + Tailwind | GPUI 0.2.2 原生 GPU 渲染 |
| 业务逻辑位置 | 散落在 `#[tauri::command]` + React 组件 | 集中在 `pairdesk-shell`（可单测） |
| 画面帧通路 | core → JSON number[] → IPC → Blob → `<img>` | core → 解码 → `RenderImage` → GPU |
| 序列化开销 | 每帧 100KB JPEG → 357KB JSON | 零序列化 |
| 前端依赖 | React/Tailwind/HeadlessUI/Vite/pnpm | 无 |
| 平台一致性 | 三端三个 WebView 引擎 | 一套布局代码，仅底层图形 API 不同 |

**不改动**：`pairdesk-core`（3770 行引擎）、`pairdesk-relay`（中继服务器）、协议、加密、传输。

## 2. 分层

```
pairdesk-ui (GPUI 壳)  ──→  pairdesk-shell (领域)  ──→  pairdesk-core (引擎)
   只画 + 只转发              业务规则 + 单测            系统 API + 网络
```

详见 [`../architecture/clean-architecture.md`](../architecture/clean-architecture.md)。

## 3. 行为等价清单

迁移后以下行为必须**逐字一致**。每项都有对应单测（见 [`test-matrix.md`](test-matrix.md)）。

### 3.1 凭据生成

| 行为 | 规格 |
|---|---|
| 密码格式 | 6 位数字，范围 `[100000, 999999]`，无前导零 |
| 会话码格式 | `pd-` + 4 位大写 base36 + 3 位数字，总长 10 |
| 复制文本 | `会话码：{sid}\n密码：{pwd}`，全角冒号，无尾随换行 |

### 3.2 会话状态机

| 阶段 | 色调 | 被控端文案 | 控制端文案 |
|---|---|---|---|
| Idle | idle | 未开始 | 未连接 |
| Authentication | connecting | 等待连接 | 连接中… |
| Authenticated | online | 等待连接 | 已认证，等待画面 |
| Connected | online | 有人正在观看 | 已连接 |
| Disconnected | idle | 未开始 | 已断开 |
| Error | error | 出错 | 出错 |

**活跃判定**：`Authentication | Authenticated | Connected`（决定显示表单还是画面）
**脉冲动画**：`Authentication | Authenticated`

### 3.3 命令校验

| 场景 | 结果 |
|---|---|
| 开关关闭时起被控端 | 拒绝，文案「允许远程控制」开关是关闭的 |
| 会话码或密码为空 | 拒绝，不改阶段 |
| 控制端起连接 | **不受**允许开关约束 |
| 活跃会话中重复连接 | 拒绝 |
| 开始新会话 | 清空 screen/error/notice/transport |

### 3.4 输入映射

| 行为 | 规格 |
|---|---|
| 坐标换算 | 先夹紧到画面矩形，再按比例放大到远端分辨率 |
| 鼠标按键 | 浏览器 0/1/2 → 协议 1/2/3，未知值当左键 |
| 修饰键位 | shift=1, ctrl=2, alt=4, meta=8 |
| 特殊键 | X11 keysym 常量（方向键、功能键、修饰键） |
| 可打印 ASCII | 直接取码点 |
| 中文/emoji/IME | 返回 0，**不发送** |

### 3.5 权限引导

| 场景 | 行为 |
|---|---|
| 非 macOS（`need_guidance=false`） | 永不显示 |
| 被控端 | 只看屏幕录制 |
| 控制端 | 只看辅助功能 |
| 首页 | 两者都看，屏幕录制在前 |
| 检测失败 | 兜底为"全都有权限"，不阻塞用户 |

### 3.6 更新检查

| 行为 | 规格 |
|---|---|
| 版本比较 | 朴素字符串比较（保留原行为，含降级误报） |
| 空 tag | 视为无需更新 |
| 资产匹配 | macOS: `aarch64.dmg`/`x86_64.dmg`；Windows: `.msi`；Linux: `.AppImage` |
| 无匹配资产 | 回落到 release 页面 URL |

### 3.7 默认值

| 项 | 值 |
|---|---|
| 允许远程控制开关 | `false`（必须用户手动打开） |
| 连接模式 | `relay` |
| 画质 | `jpeg: 80, fps: 20` |
| 中继地址 | `35.209.148.235:8977` |
| 打洞端口 | `23517` |
| 窗口尺寸 | 980 × 680 |

## 4. 验收标准

- [ ] `cargo test -p pairdesk-shell` 全绿（81 个单测）
- [ ] `cargo test --lib -p pairdesk-core` 全绿（引擎零回归）
- [ ] `cargo build -p pairdesk-ui` 成功（三平台 CI）
- [ ] macOS 真机：两台机器能互相连上并看到画面
- [ ] macOS 真机：鼠标键盘注入生效，中文输入不产生脏键码
- [ ] 20fps 稳态 CPU < 30%，RSS < 300MB
- [ ] 权限引导在首次运行时正确弹出并可跳转系统设置
- [ ] 更新检查能正确识别新版本并下载对应平台安装包

## 5. 非目标

- ❌ 不改协议、不改加密、不改传输策略
- ❌ 不新增功能（不加重连、不加历史记录）
- ❌ 不优化 UI 视觉设计（1:1 复刻现有布局）
- ❌ Windows 后端（M4）不在本次范围内——它依赖 `pairdesk-core`，与 UI 无关
