/**
 * 通用按钮。
 *
 * 变体语义对应操作性质：
 *  - primary：主操作（如"开始接收协助"）
 *  - secondary：次要操作（默认灰底）
 *  - danger：危险操作（如"断开连接"）
 *  - ghost：弱化操作（如"取消"、"返回"）
 *
 * 该组件为纯展示控件：不持有状态，状态由上层注入（loading/disabled/value）。
 */
import type { ButtonHTMLAttributes, ReactNode } from 'react';

export type ButtonVariant = 'primary' | 'secondary' | 'danger' | 'ghost';
export type ButtonSize = 'sm' | 'md' | 'lg';

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** 视觉变体 */
  variant?: ButtonVariant;
  /** 尺寸（默认 md） */
  size?: ButtonSize;
  /** 是否显示加载态（禁用点击） */
  loading?: boolean;
  children: ReactNode;
}

export function Button({
  variant = 'secondary',
  size = 'md',
  loading = false,
  disabled,
  className,
  children,
  type = 'button',
  ...rest
}: ButtonProps) {
  // 类名：基础类 + 变体类 + 尺寸类；透传外部 className 便于 app 层覆盖
  const cls = [
    'pd-btn',
    `pd-btn--${variant}`,
    `pd-btn--${size}`,
    className ?? '',
  ]
    .join(' ')
    .trim();

  return (
    <button
      {...rest}
      type={type}
      className={cls}
      disabled={disabled || loading}
      aria-busy={loading}
    >
      {loading ? '…' : children}
    </button>
  );
}
