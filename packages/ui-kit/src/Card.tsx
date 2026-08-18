/**
 * 卡片容器：把相关内容分组并给出统一底色/圆角/内边距。
 * 仅提供容器，不做任何业务逻辑；children 由调用方自由组装。
 */
import type { HTMLAttributes, ReactNode } from 'react';

export interface CardProps extends HTMLAttributes<HTMLDivElement> {
  children: ReactNode;
}

export function Card({ className, children, ...rest }: CardProps) {
  return (
    <div className={`pd-card ${className ?? ''}`.trim()} {...rest}>
      {children}
    </div>
  );
}
