/**
 * 桥接层的类型定义：与后端 `pairdesk-core` 的 CoreEvent / IPC 命令一一对齐。
 *
 * 这是一份"契约"：无论跑在 Tauri 里（真实后端）还是浏览器（mock），
 * 前端业务层只认这里的统一形状。
 *
 * 用户模型：被控端生成【会话码 + 密码】并打开「允许远程控制」；
 * 控制端输入【会话码 + 密码】，程序自动择一（同网直连 / QUIC 打洞 / 中继兜底）。
 */

/** 后端 CoreEvent 的镜像（type 字段区分事件类别，字段驼峰化） */
export type CoreEvent =
  | { type: 'frame'; jpeg: number[] } // 一帧远程画面(JPEG 字节)
  | { type: 'size'; w: number; h: number } // 远端屏幕分辨率
  | { type: 'peerConnected' }
  | { type: 'peerDisconnected' }
  | { type: 'authResult'; ok: boolean; reason?: string } // 认证结果
  | { type: 'error'; message: string }
  | { type: 'notice'; message: string } // 非致命提示（如打洞端口顺延）
  | { type: 'stats'; fps: number; kbps: number; pingMs: number }
  | { type: 'transport'; path: string } // 自动择一选中的传输路径
  | { type: 'signalHole'; addr: string }; // 信令打洞端点

/** 输入消息（与 Rust InputMsg 对齐；控制端 → 被控端注入） */
export type InputMsg =
  | { kind: 'mouse-move'; x: number; y: number }
  | { kind: 'button'; btn: number; down: boolean }
  | { kind: 'scroll'; dx: number; dy: number }
  | { kind: 'key'; keycode: number; down: boolean; mods: number };

/** 启动被控端参数 */
export interface HostParams {
  relay: string; // 中继/VPS 地址（打洞信令 + 兜底）
  sid: string; // 会话码（标识这台被控端本次会话）
  holePort: number; // 打洞 QUIC 端口
  password: string; // 连接密码
}

/** 启动控制端参数 */
export interface ConnectParams {
  relay: string;
  sid: string; // 对方给的会话码
  password: string;
}

/** 桥接层对前端暴露的统一接口（业务层只依赖这个接口） */
export interface CoreBridge {
  /** 设置"允许远程控制"总开关（关掉则拒绝起被控端） */
  setAllowed(allowed: boolean): Promise<void>;
  /** 启动被控端（自动就绪：QUIC 打洞 + 中继兜底） */
  startHostAuto(p: HostParams): Promise<void>;
  /** 启动控制端（自动择一：QUIC 打洞优先 → 中继兜底） */
  connectAuto(p: ConnectParams): Promise<void>;
  /** 主动断开/停止 */
  stop(): void;
  /** 发送输入事件（控制端 → 被控端） */
  sendInput(msg: InputMsg): void;
  /** 订阅核心事件流；返回取消订阅函数 */
  onEvent(cb: (e: CoreEvent) => void): () => void;
}
