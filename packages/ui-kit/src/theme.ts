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
  idle: '#9aa0a6', // 空闲/未连接
  connecting: '#f59e0b', // 连接中/认证中（琥珀）
  online: '#22c55e', // 在线/已连接（绿）
  error: '#ef4444', // 错误（红）
} as const;

/** 强调色（主操作）与危险色 */
export const ACCENT = {
  primary: '#3b82f6', // 蓝，主操作按钮
  primaryHover: '#2563eb',
  danger: '#ef4444', // 危险操作（断开等）
  dangerHover: '#dc2626',
} as const;

/** 字体/布局基准（rem 固定，不随系统字号膨胀——对齐 md-editor 的控件尺寸纪律） */
export const SIZE = {
  fontSize: {
    sm: '0.8125rem', // 13px 辅助文字
    md: '0.9375rem', // 15px 正文
    lg: '1.125rem', // 18px 标题
    xl: '1.5rem', // 24px 大标题/密码
  },
  radius: {
    sm: '0.375rem', // 6px 小控件
    md: '0.5rem', // 8px 输入框/卡片
    lg: '0.75rem', // 12px 大卡片
  },
  spacing: {
    xs: '0.25rem',
    sm: '0.5rem',
    md: '0.75rem',
    lg: '1rem',
    xl: '1.5rem',
  },
  control: {
    height: '2.25rem', // 36px 标准控件高度
  },
} as const;

/** 阴影层级 */
export const SHADOW = {
  card: '0 1px 3px rgba(0,0,0,0.1)',
  popup: '0 8px 24px rgba(0,0,0,0.18)',
} as const;
