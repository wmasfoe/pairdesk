/**
 * Mock 桥接实现：纯浏览器模拟（用于 Vite 预览 / 前端开发调试）。
 *
 * 不发起任何真实网络连接，而是模拟"认证成功 → 收画面帧"的事件流，
 * 用程序生成渐变 JPEG（验证画面渲染链路），脱离 Tauri 也能开发 UI。
 */
import type { CoreBridge, CoreEvent } from './types';

const listeners = new Set<(e: CoreEvent) => void>();
let timers: ReturnType<typeof setInterval>[] = [];

export function createMockBridge(): CoreBridge {
  return {
    async setAllowed() {
      // mock 下忽略开关
    },
    async startHostAuto() {
      // 模拟被控端：就绪
      emit({ type: 'authResult', ok: true });
      emit({ type: 'peerConnected' });
    },
    async connectAuto() {
      // 模拟控制端：认证 + 收帧
      emit({ type: 'authResult', ok: true });
      emit({ type: 'peerConnected' });
      emit({ type: 'size', w: 1280, h: 800 });
      timers.push(setInterval(() => emitFakeFrame(), 120));
    },
    stop() {
      timers.forEach(clearInterval);
      timers = [];
      emit({ type: 'peerDisconnected' });
    },
    sendInput() {
      // mock 下忽略输入
    },
    onEvent(cb) {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
  };
}

function emit(e: CoreEvent) {
  listeners.forEach((l) => l(e));
}

/** 生成一张简单的渐变 JPEG 帧，模拟远程画面 */
function emitFakeFrame() {
  const w = 1024;
  const h = 640;
  const canvas = document.createElement('canvas');
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext('2d')!;
  const grad = ctx.createLinearGradient(0, 0, w, h);
  grad.addColorStop(0, '#3b82f6');
  grad.addColorStop(1, '#22c55e');
  ctx.fillStyle = grad;
  ctx.fillRect(0, 0, w, h);
  ctx.fillStyle = '#1f2937';
  ctx.fillRect(0, 0, w, 40);
  ctx.fillStyle = '#fff';
  ctx.font = '28px sans-serif';
  ctx.fillText('PairDesk 远程画面 (Mock)', 16, 90);

  canvas.toBlob(
    (blob) => {
      if (!blob) return;
      void blob.arrayBuffer().then((buf) => {
        emit({ type: 'frame', jpeg: Array.from(new Uint8Array(buf)) });
      });
    },
    'image/jpeg',
    0.8,
  );
}
