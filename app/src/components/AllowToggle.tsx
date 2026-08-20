import { Description, Field, Label, Switch } from '@headlessui/react';
import { cn } from '../lib/cn';

export function AllowToggle({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <Field className="flex items-center justify-between gap-4 rounded-pd-lg border border-pd-border bg-pd-elev px-4 py-3">
      <div className="min-w-0">
        <Label className="block text-sm font-medium text-pd-fg">允许远程控制</Label>
        <Description className="text-xs leading-snug text-pd-muted">关闭后即使有码也无法连入</Description>
      </div>
      <Switch
        checked={checked}
        onChange={onChange}
        className={cn(
          'group relative inline-flex h-6 w-11 shrink-0 cursor-pointer rounded-full p-1 transition',
          'bg-white/15 data-checked:bg-pd-primary',
          'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-pd-primary',
        )}
      >
        <span className="pointer-events-none size-4 rounded-full bg-white shadow transition group-data-checked:translate-x-5" />
      </Switch>
    </Field>
  );
}
