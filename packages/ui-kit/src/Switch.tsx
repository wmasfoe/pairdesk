/**
 * 开关：标签 + 描述 + 轨道。
 *
 * 纯受控；checked / onChange 由上层注入。
 */
import type { ReactNode } from 'react';

export interface SwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: ReactNode;
  description?: ReactNode;
  disabled?: boolean;
}

export function Switch({ checked, onChange, label, description, disabled }: SwitchProps) {
  return (
    <label className="pd-toggle">
      <span className="pd-toggle__text">
        <span className="pd-toggle__label">{label}</span>
        {description ? <span className="pd-toggle__desc">{description}</span> : null}
      </span>
      <input
        type="checkbox"
        className="pd-toggle__input"
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span className="pd-toggle__track" aria-hidden />
    </label>
  );
}
