/**
 * 界面用线性图标（描边 1.75，对齐 24 视口）。
 * 不引入图标库，保持 ui-kit 零依赖。
 */
import type { SVGProps } from 'react';

type IconProps = SVGProps<SVGSVGElement> & { size?: number };

function Svg({ size = 18, children, ...rest }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.75"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
      {...rest}
    >
      {children}
    </svg>
  );
}

export function IconMonitor(p: IconProps) {
  return (
    <Svg {...p}>
      <rect x="3" y="4" width="18" height="12" rx="2" />
      <path d="M8 20h8M12 16v4" />
    </Svg>
  );
}

export function IconPointer(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M5 4.5 19 12.2l-6.2 1.4L11 20.5z" />
    </Svg>
  );
}

export function IconCopy(p: IconProps) {
  return (
    <Svg {...p}>
      <rect x="9" y="9" width="11" height="11" rx="2" />
      <path d="M5 15V7a2 2 0 0 1 2-2h8" />
    </Svg>
  );
}

export function IconRefresh(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M20 12a8 8 0 1 1-2.3-5.6" />
      <path d="M20 4v6h-6" />
    </Svg>
  );
}

export function IconCheck(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M5 12.5 9.5 17 19 7.5" />
    </Svg>
  );
}

export function IconShield(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="M12 3 5 6.5v5.3c0 4 2.8 6.8 7 8.7 4.2-1.9 7-4.7 7-8.7V6.5L12 3z" />
    </Svg>
  );
}

export function IconScreen(p: IconProps) {
  return (
    <Svg {...p}>
      <rect x="3" y="5" width="18" height="12" rx="2" />
      <path d="M8 21h8" />
    </Svg>
  );
}

export function IconKeyboard(p: IconProps) {
  return (
    <Svg {...p}>
      <rect x="3" y="7" width="18" height="11" rx="2" />
      <path d="M7 11h.01M11 11h.01M15 11h.01M8 15h8" />
    </Svg>
  );
}

export function IconChevron(p: IconProps) {
  return (
    <Svg {...p}>
      <path d="m9 6 6 6-6 6" />
    </Svg>
  );
}
