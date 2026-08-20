import type { ReactNode } from 'react';
import { Button } from '@pairdesk/ui-kit';

export function PageHeader({
  title,
  onBack,
  children,
}: {
  title: string;
  onBack: () => void;
  children?: ReactNode;
}) {
  return (
    <header className="flex items-center gap-3">
      <Button variant="ghost" size="sm" onClick={onBack}>
        ← 返回
      </Button>
      <h1 className="flex-1 font-display text-base font-semibold tracking-tight text-pd-fg">{title}</h1>
      {children}
    </header>
  );
}
