/**
 * 凭据展示行：会话码 / 密码等需要「大字 + 复制 + 换一个」的字段。
 */
import { useState } from 'react';
import { Button } from '@pairdesk/ui-kit';
import { IconCheck, IconCopy, IconRefresh } from './icons';
import { cn } from '../lib/cn';

export function Credential({
  label,
  value,
  onChange,
  onRefresh,
  large = false,
  mono = true,
}: {
  label: string;
  value: string;
  onChange?: (v: string) => void;
  onRefresh?: () => void;
  large?: boolean;
  mono?: boolean;
}) {
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1400);
    } catch {
      /* clipboard 在部分环境不可用，忽略 */
    }
  };

  return (
    <div className="flex flex-col gap-1.5">
      <div className="text-xs font-medium tracking-wide text-pd-muted">{label}</div>
      <div className="flex items-center gap-1.5">
        <input
          className={cn(
            'min-w-0 flex-1 rounded-pd border border-pd-border bg-pd-elev px-3.5 text-pd-fg outline-none',
            'focus:border-pd-primary focus:shadow-[0_0_0_3px_var(--color-pd-primary-soft)]',
            large ? 'h-[3.25rem] text-center text-[1.375rem] font-semibold tracking-[0.12em]' : 'h-11 text-base',
            mono && 'font-mono tabular-nums tracking-wider',
          )}
          value={value}
          onChange={(e) => onChange?.(e.target.value)}
          spellCheck={false}
          autoComplete="off"
        />
        <Button className="pd-iconbtn" variant="ghost" size="sm" onClick={copy} title="复制" aria-label="复制">
          {copied ? <IconCheck /> : <IconCopy />}
        </Button>
        {onRefresh && (
          <Button className="pd-iconbtn" variant="ghost" size="sm" onClick={onRefresh} title="换一个" aria-label="换一个">
            <IconRefresh />
          </Button>
        )}
      </div>
    </div>
  );
}
