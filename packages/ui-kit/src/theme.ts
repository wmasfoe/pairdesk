/**
 * 主题设计规范（设计 token）。
 *
 * 双通道方案：
 *  1. `styles.css` 用 CSS 变量承载颜色/尺寸，组件通过 `var(--pd-*)` 引用，
 *     这样切换亮/暗主题时只换根类名（.pd-light / .pd-dark），零重渲染开销。
 *  2. 本文件导出一份 TypeScript 常量，供需要"读取颜色做逻辑判断"的场合
 *     （例如状态色映射、动态计算），保持与 CSS 变量同源。
 *
 * 命名：颜色语义化（fg=前景/bg=背景/border=边框/primary=主色…），
 *       不暴露具体色值，保证未来换色板时组件零改动。
 */

/** 状态语义色（供 StatusDot / 连接状态等使用） */
export const STATUS_COLORS = {
  idle: '#6b7280',
  connecting: '#f59e0b',
  online: '#34d399',
  error: '#f87171',
} as const;

/** 强调色（主操作）与危险色 —— teal 系，与 logo 青色信号统一 */
export const ACCENT = {
  primary: '#2dd4bf',
  primaryHover: '#5eead4',
  danger: '#f87171',
  dangerHover: '#fca5a5',
} as const;

/** 字体/布局基准（rem 固定，不随系统字号膨胀） */
export const SIZE = {
  fontSize: {
    sm: '0.8125rem',
    md: '0.9375rem',
    lg: '1.125rem',
    xl: '1.5rem',
  },
  radius: {
    sm: '0.375rem',
    md: '0.625rem',
    lg: '0.875rem',
  },
  spacing: {
    xs: '0.25rem',
    sm: '0.5rem',
    md: '0.75rem',
    lg: '1rem',
    xl: '1.5rem',
  },
  control: {
    height: '2.5rem',
  },
} as const;

/** 阴影层级 */
export const SHADOW = {
  card: '0 8px 24px rgba(0,0,0,0.18)',
  popup: '0 16px 40px rgba(0,0,0,0.4)',
} as const;
