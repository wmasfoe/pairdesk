/**
 * 状态指示点：色点 + 文字。
 *
 * 用于连接/会话状态展示（空闲/连接中/在线/错误）。
 * tone 决定色点颜色，文案由调用方给出（保持组件无知化）。
 */
import type { ReactNode } from 'react';
import { STATUS_COLORS } from './theme';

export type StatusTone = keyof typeof STATUS_COLORS;

export interface StatusDotProps {
  /** 状态色调 */
  tone: StatusTone;
  /** 状态文案 */
  children: ReactNode;
  /** 是否显示脉冲动画（如连接中） */
  pulse?: boolean;
}

export function StatusDot({ tone, children, pulse = false }: StatusDotProps) {
  const color = STATUS_COLORS[tone];
  return (
    <span className="pd-status">
      {/* 内联色值来自设计 token；pulse 加呼吸动画提示进行中 */}
      <span
        className="pd-status__dot"
        style={{
          background: color,
          animation: pulse
            ? 'pd-pulse 1.2s ease-in-out infinite'
            : undefined,
        }}
      />
      {children}
    </span>
  );
}
