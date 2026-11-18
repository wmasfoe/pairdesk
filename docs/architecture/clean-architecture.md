# 整洁架构规则

> 这份文档回答：**代码放哪一层，依赖可以指向谁，什么绝对不能做。**

## 1. 分层与依赖方向

```
┌─────────────────────────────────────────────────────────────┐
│  适配器层（可替换，可并行存在多套）                            │
│                                                             │
│   crates/pairdesk-app (Tauri 2 壳)     ← 现有，迁移后移除      │
│   crates/pairdesk-ui  (GPUI 壳)        ← 新建，目标形态        │
│                                                             │
│   职责：把 SessionState 渲染成像素；把鼠标键盘转成命令          │
│   禁止：业务判断、状态推导、坐标换算、按键映射                  │
└───────────────────────────┬─────────────────────────────────┘
                            │ 依赖（单向，只能向下）
┌───────────────────────────▼─────────────────────────────────┐
│  领域层  crates/pairdesk-shell                                │
│                                                             │
│   会话状态机 / 凭据生成 / 坐标换算 / 按键映射 / 权限视图模型     │
│   更新检查 / 内核事件 → 视图事件映射                           │
│                                                             │
│   纯 Rust，零 UI 框架依赖，100% 可 cargo test                  │
└───────────────────────────┬─────────────────────────────────┘
                            │ 依赖（单向，只能向下）
┌───────────────────────────▼─────────────────────────────────┐
│  引擎层  crates/pairdesk-core                                 │
│                                                             │
│   协议 / 采集 / 编码 / 加密 / 传输 / 打洞 / 中继 / 输入注入      │
│   对外只暴露 CoreHandle（命令） + CoreEvent（事件流）           │
│   不知道"界面"存在                                            │
└─────────────────────────────────────────────────────────────┘

  crates/pairdesk-relay  ← 独立部署，不打包进客户端，不参与上述依赖
```

## 2. 各层职责与红线

### 2.1 引擎层 `pairdesk-core`

**职责**：一切与操作系统和网络打交道的脏活。

| 模块 | 职责 |
|---|---|
| `protocol` | 帧协议、握手、XChaCha20-Poly1305 加解密 |
| `transport` / `relay` / `punch` / `quic_frame` | TCP 直连、中继桥接、QUIC 打洞、自动择优 |
| `capture` | 屏幕采集（macOS ScreenCaptureKit / Windows GDI / Linux X11） |
| `input` | 输入注入（CGEvent / SendInput / XTest） |
| `session` | 会话主循环，产出 `CoreEvent` |
| `encode` | JPEG 编码 |

**红线**：
- ❌ 不引用任何 UI 框架（Tauri、GPUI、winit、egui…）
- ❌ 不出现"窗口""按钮""页面"等概念
- ✅ 对外接口只有 `CoreHandle` + `Receiver<CoreEvent>`

### 2.2 领域层 `pairdesk-shell`

**职责**：应用的业务规则——**这部分决定 App 行为是否正确**。

| 模块 | 职责 | 迁移来源 |
|---|---|---|
| `session` | 会话状态机：idle → authentication → authenticated → connected | `useSession.ts` + `bridge.rs` |
| `credentials` | 会话码/密码生成、复制文本格式 | `HostPage.tsx` |
| `input` | 鼠标坐标换算、按键 → X11 keysym、修饰键位 | `VideoView.tsx` |
| `permissions` | 权限引导的展示判定 | `PermissionBanner.tsx` |
| `update` | 版本比较、平台资产匹配 | `bridge.rs::pd_check_update` |
| `event` | 内核事件 → 视图事件的映射 | `bridge.rs::core_event_to_json` |

**红线**：
- ❌ 不依赖 Tauri、GPUI、React、任何渲染库
- ❌ 不直接调用平台 API（权限、剪贴板、进程重启）——只**描述意图**，由适配器执行
- ❌ 不做 I/O（网络、文件）——只做纯计算与状态转换
- ✅ 每个公开函数/状态转换**必须有单测**
- ✅ 允许依赖 `pairdesk-core` 的**类型**（`CoreEvent`、`Quality`、`InputMsg`）

### 2.3 适配器层 `pairdesk-app` / `pairdesk-ui`

**职责**：只做两件事——**渲染**与**转发**。

| 允许 | 禁止 |
|---|---|
| 把 `SessionState` 画成像素 | 自己推导会话阶段 |
| 把鼠标位置交给 `shell::input` 换算 | 自己写坐标换算公式 |
| 把按键事件交给 `shell::input` 映射 | 自己写 keysym 表 |
| 调用 `shell` 返回的"意图"去执行系统动作 | 自己判断该不该弹权限引导 |
| 处理窗口生命周期、GPU 纹理、IME | 处理会话业务逻辑 |

**判定方法**：把 UI 层删掉，`shell` + `core` 的单测应该仍然全绿，且能完整描述 App 行为。

## 3. 跨层通信契约

### 3.1 向下：命令

```rust
// 适配器 → shell
let plan = shell::session::SessionState::start_host(&mut state, params)?;
// plan 是「意图」，适配器负责真正执行
match plan {
    Intent::StartHost { mode, relay, sid, hole_port, password } => {
        CoreHandle::start_host_auto(relay, sid, hole_port, password)  // 引擎层
    }
    Intent::Reject { reason } => { /* 只更新 UI 显示 */ }
}
```

### 3.2 向上：事件

```rust
// 引擎 → shell → 适配器
let view_event = shell::event::from_core(core_event);   // 纯映射，可单测
match view_event {
    ViewEvent::Frame { jpeg } => render(jpeg),
    ViewEvent::PhaseChanged(p) => update_status_dot(p),
    _ => {}
}
```

## 4. 新增代码时的判定流程

```
新需求来了
    │
    ├─ 涉及系统 API / 网络 / 采集 / 注入？
    │      └→ pairdesk-core（引擎层）
    │
    ├─ 是业务规则、状态转换、数据计算？
    │      └→ pairdesk-shell（领域层）+ 立刻写单测
    │
    └─ 只是"怎么画出来"或"用户点了怎么转成调用"？
           └→ 适配器层（pairdesk-ui / pairdesk-app）
```

## 5. 为什么这样分

1. **可测**：现在业务逻辑锁在 `#[tauri::command]` 和 React 组件里，必须起完整运行时才能验证。抽到 `shell` 后 `cargo test` 秒级跑完。
2. **可换**：从 Tauri 换到 GPUI，只需重写最外层。`shell` 的 100+ 个单测是重构的护栏——任何行为偏差立刻红灯。
3. **可并行**：两套 UI 壳可以同时存在（Tauri 版继续发 Windows，GPUI 版先上 macOS），共享同一个 `shell` + `core`。
