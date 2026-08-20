/**
 * Tauri 原生桥接实现：把前端调用翻译为 Tauri 命令 + 事件。
 *
 * 对应后端 `crates/pairdesk-app/src/bridge.rs`：
 *  - invoke('pd_start_host_auto' / 'pd_connect_auto' / 'pd_set_allowed' / ...)
 *  - 后端通过 listen('core://event') 把 CoreEvent 推给前端
 */
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  ConnectParams,
  CoreBridge,
  CoreEvent,
  HostParams,
  InputMsg,
} from './types';

/** 前端监听者集合（模块级：getCoreBridge 单实例，监听只注册一次） */
const listeners = new Set<(e: CoreEvent) => void>();

export function createTauriBridge(): CoreBridge {
  let unlisten: UnlistenFn | null = null;

  return {
    async setAllowed(allowed) {
      await invoke('pd_set_allowed', { allowed });
    },
    async startHostAuto({ relay, sid, holePort, password }: HostParams) {
      await invoke('pd_start_host_auto', { relay, sid, holePort, password });
    },
    async connectAuto({ relay, sid, password }: ConnectParams) {
      await invoke('pd_connect_auto', { relay, sid, password });
    },
    stop() {
      void invoke('pd_stop');
    },
    sendInput(msg) {
      void invoke('pd_send_input', { msg: serializeInput(msg) });
    },
    async checkPermissions() {
      const res = await invoke<{ screen_recording: boolean; accessibility: boolean; need_guidance: boolean }>('pd_check_permissions');
      return {
        screenRecording: res.screen_recording,
        accessibility: res.accessibility,
        needGuidance: res.need_guidance,
      };
    },
    async requestPermission(type) {
      return await invoke<boolean>('pd_request_permission', { permissionType: type });
    },
    async openPermissionSettings(type) {
      await invoke('pd_open_permission_settings', { permissionType: type });
    },
    restartApp() {
      void invoke('pd_restart_app');
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

/** 把前端 InputMsg 序列化为后端可识别的扁平结构（kind 为 Rust 变体名） */
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
