/**
 * Tauri 原生桥接实现：把前端调用翻译为 Tauri 命令 + 事件。
 *
 * 对应后端 `crates/pairdesk-app/src/bridge.rs`：
 *  - invoke('pd_connect', {...}) → 后端调用 pairdesk-core::CoreHandle
 *  - 后端通过 listen('core://event') 把 CoreEvent 推给前端
 */
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { CoreBridge, CoreEvent, InputMsg } from './types';

/** 前端监听者集合（模块级：getCoreBridge 单实例，监听只注册一次） */
const listeners = new Set<(e: CoreEvent) => void>();

export function createTauriBridge(): CoreBridge {
  let unlisten: UnlistenFn | null = null;

  return {
    async startHost(port, password) {
      await invoke('pd_start_host', { port, password });
    },
    async connect(addr, password) {
      await invoke('pd_connect', { addr, password });
    },
    stop() {
      void invoke('pd_stop');
    },
    sendInput(msg) {
      void invoke('pd_send_input', { msg: serializeInput(msg) });
    },
    onEvent(cb) {
      // 幂等注册：只在首次订阅时建立与后端的监听，多订阅方走 Set 分发
      if (!unlisten) {
        void listen<CoreEvent>('core://event', (ev) => {
          listeners.forEach((l) => l(ev.payload));
        }).then((fn) => (unlisten = fn));
      }
      listeners.add(cb);
      return () => {
        listeners.delete(cb);
      };
    },
  };
}

/** 把前端 InputMsg 序列化为后端可识别的扁平结构 */
function serializeInput(msg: InputMsg): object {
  switch (msg.kind) {
    case 'mouse-move':
      return { kind: 'MouseMove', x: msg.x, y: msg.y };
    case 'button':
      return { kind: 'Button', btn: msg.btn, down: msg.down };
    case 'scroll':
      return { kind: 'Scroll', dx: msg.dx, dy: msg.dy };
    case 'key':
      return { kind: 'Key', keycode: msg.keycode, down: msg.down, mods: msg.mods };
  }
}
