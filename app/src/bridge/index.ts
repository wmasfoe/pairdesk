/**
 * 桥接层入口：按运行环境同步选择适配器。
 *
 *  - Tauri 环境（window.__TAURI_INTERNALS__ 存在）→ 原生桥接（真实能力）
 *  - 纯浏览器 / Vite 预览 → Mock 桥接（仅 UI 预览，无真实网络）
 *
 * @tauri-apps/api 是纯 JS 包，在浏览器中 import 不会崩溃（只有真正调用
 * invoke/listen 才需要 Tauri 运行时），故可安全静态导入。
 */
import { createTauriBridge } from './tauri';
import { createMockBridge } from './mock';
import type { CoreBridge } from './types';

let cached: CoreBridge | null = null;

export function getCoreBridge(): CoreBridge {
  if (!cached) {
    const isTauri =
      typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
    cached = isTauri ? createTauriBridge() : createMockBridge();
  }
  return cached;
}
