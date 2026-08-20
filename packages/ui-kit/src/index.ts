/**
 * ui-kit 公共导出面（收窄：只导出对外组件/类型，内部实现细节不外泄）。
 *
 * 依赖方向：ui-kit 不 import 任何业务代码；app 通过本入口引用组件库。
 */
export * from './theme';

export { Button } from './Button';
export type { ButtonProps, ButtonVariant, ButtonSize } from './Button';

export { TextField } from './TextField';
export type { TextFieldProps } from './TextField';

export { StatusDot } from './StatusDot';
export type { StatusDotProps, StatusTone } from './StatusDot';

export { Toast } from './Toast';
export type { ToastProps, ToastTone } from './Toast';

export { Card } from './Card';
export type { CardProps } from './Card';

export { Switch } from './Switch';
export type { SwitchProps } from './Switch';
