/**
 * 桥接层的类型定义：与后端 `pairdesk-core` 的 CoreEvent / 命令一一对齐。
 *
 * 这份类型是"契约"：无论实际跑在 Tauri 里还是浏览器 mock，
 * 前端业务层只认识这里定义的统一形状，不感知底层实现。
 */

/** 后端 CoreEvent 的镜像（字段与 Rust 侧一致，命名驼峰化） */
export type CoreEvent =
  | { type: 'screen-frame'; jpeg: Uint8Array } // 一帧远程画面(JPEG)
  | { type: 'size'; w: number; h: number } // 远端屏幕分辨率
  | { type: 'peer-connected' } // 对端已建立连接
  | { type: 'peer-disconnected' } // 对端断开
  | { type: 'auth-result'; ok: boolean; reason?: string } // 认证结果
  | { type: 'error'; message: string } // 错误
  | { type: 'stats'; fps: number; kbps: number; pingMs: number }; // 统计

/** 输入消息（与 Rust InputMsg 对齐；控制端转发给被控端注入） */
export type InputMsg =
  | { kind: 'mouse-move'; x: number; y: number }
  | { kind: 'button'; btn: number; down: boolean }
  | { kind: 'scroll'; dx: number; dy: number }
  | { kind: 'key'; keycode: number; down: boolean; mods: number };

/** 桥接层对前端暴露的统一接口（业务层只依赖这个接口） */
export interface CoreBridge {
  /** 启动被控端：监听端口等待连接 */
  startHost(port: number, password: string): Promise<void>;
  /** 启动控制端：连接远端 */
  connect(addr: string, password: string): Promise<void>;
  /** 主动断开/停止 */
  stop(): void;
  /** 发送输入事件（控制端 → 被控端） */
  sendInput(msg: InputMsg): void;
  /** 订阅核心事件流；返回取消订阅函数 */
  onEvent(cb: (e: CoreEvent) => void): () => void;
}
