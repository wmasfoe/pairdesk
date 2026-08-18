/**
 * 轻提示 Toast。
 *
 * 短暂引导性提示，自动消失（由上层管理生命周期与定时）。
 * 纯展示：不持内部计时器，避免组件库暗自产生副作用——计时由调用侧控制。
 */
import type { ReactNode } from 'react';

export type ToastTone = 'info' | 'success' | 'error';

export interface ToastProps {
  tone?: ToastTone;
  children: ReactNode;
}

export function Toast({ tone = 'info', children }: ToastProps) {
  // 色调仅影响左边 3px 色条（细节点缀，符合"组件轻量"原则）
  const barColor =
    tone === 'error' ? '#ef4444' : tone === 'success' ? '#22c55e' : '#3b82f6';
  return (
    <div className="pd-toast" style={{ borderLeft: `3px solid ${barColor}` }}>
      {children}
    </div>
  );
}
