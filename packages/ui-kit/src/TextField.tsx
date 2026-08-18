/**
 * 文本输入框。
 *
 * 受控组件（需由上层提供 value/onChange）。
 * 错误信息展示于输入框下方，红色提示；无错误时不占位（用条件渲染，不撑高布局）。
 */
import type { ChangeEvent, InputHTMLAttributes } from 'react';

export interface TextFieldProps
  extends Omit<InputHTMLAttributes<HTMLInputElement>, 'onChange'> {
  /** 标签文本 */
  label?: string;
  /** 错误提示（非空时显示红色） */
  error?: string;
  /** 值变化回调 */
  onChange?: (value: string, e: ChangeEvent<HTMLInputElement>) => void;
}

export function TextField({
  label,
  error,
  onChange,
  className,
  ...rest
}: TextFieldProps) {
  return (
    <div className={`pd-field ${className ?? ''}`.trim()}>
      {label && <label className="pd-field__label">{label}</label>}
      <input
        {...rest}
        className="pd-field__input"
        aria-invalid={!!error}
        onChange={(e) => onChange?.(e.target.value, e)}
      />
      {error && <span className="pd-field__error">{error}</span>}
    </div>
  );
}
