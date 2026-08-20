import type { ReactNode } from 'react';
import { Disclosure, DisclosureButton, DisclosurePanel } from '@headlessui/react';
import { cn } from '../lib/cn';
import { IconChevron } from './icons';

export function AdvancedPanel({ children }: { children: ReactNode }) {
  return (
    <Disclosure as="div" className="rounded-pd border border-pd-border">
      {({ open }) => (
        <>
          <DisclosureButton className="flex w-full items-center gap-2 px-3.5 py-2.5 text-left text-[13px] text-pd-muted transition hover:text-pd-fg">
            <IconChevron
              size={14}
              className={cn('transition-transform', open ? 'rotate-90' : '')}
            />
            高级设置
          </DisclosureButton>
          <DisclosurePanel className="flex flex-col gap-3.5 px-3.5 pb-3.5">{children}</DisclosurePanel>
        </>
      )}
    </Disclosure>
  );
}
