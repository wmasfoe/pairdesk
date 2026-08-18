/**
 * Mock 桥接实现：纯浏览器模拟（用于 Vite 预览 / 前端开发调试）。
 *
 * 不会发起任何真实网络连接，而是：
 *  - 模拟"连接成功 → 收画面帧"的事件流
 *  - 用程序生成一个纯色/渐变的 JPEG 帧（验证画面渲染链路）
 *
 * 这样脱离 Tauri 运行时也能看到并开发 UI，后续接上真实后端无感切换。
 */
import type { CoreBridge, CoreEvent } from './types';

const listeners = new Set<(e: CoreEvent) => void>();
let timers: ReturnType<typeof setInterval>[] = [];

export function createMockBridge(): CoreBridge {
  return {
    async startHost(port, password) {
      // 模拟被控端：广播已就绪
      emit({ type: 'auth-result', ok: true });
      emit({ type: 'peer-connected' });
    },
    async connect(addr, password) {
      // 模拟控制端：认证 + 收帧
      emit({ type: 'auth-result', ok: true });
      emit({ type: 'peer-connected' });
      emit({ type: 'size', w: 1280, h: 800 });
      // 周期推帧
      timers.push(setInterval(() => emitFakeFrame(), 120));
    },
    stop() {
      timers.forEach(clearInterval);
      timers = [];
      emit({ type: 'peer-disconnected' });
    },
    sendInput() {
      // mock 下忽略输入（无真实对端）
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

/** 生成一张简单的渐变 JPEG 帧（1024x640），模拟远程画面 */
function emitFakeFrame() {
  const w = 1024;
  const h = 640;
  const canvas = document.createElement('canvas');
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext('2d')!;
  // 顶部一条"桌面栏"+ 底色，制造视觉内容
  const grad = ctx.createLinearGradient(0, 0, w, h);
  grad.addColorStop(0, '#3b82f6');
  grad.addColorStop(1, '#22c55e');
  ctx.fillStyle = grad;
  ctx.fillRect(0, 0, w, h);
  ctx.fillStyle = '#1f2937';
  ctx.fillRect(0, 0, w, 40); // 顶栏
  ctx.fillStyle = '#fff';
  ctx.font = '28px sans-serif';
  ctx.fillText('PairDesk 远程画面 (Mock)', 16, 90);

  canvas.toBlob(
    (blob) => {
      if (!blob) return;
      void blob.arrayBuffer().then((buf) => {
        emit({ type: 'screen-frame', jpeg: new Uint8Array(buf) });
      });
    },
    'image/jpeg',
    0.8,
  );
}
