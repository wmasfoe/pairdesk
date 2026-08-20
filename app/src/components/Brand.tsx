import logo from '../assets/cat-signal.png';
import { cn } from '../lib/cn';

export function Brand({ compact = false }: { compact?: boolean }) {
  return (
    <div className="inline-flex items-center gap-2.5">
      <img
        className="block rounded-[10px]"
        src={logo}
        alt="PairDesk"
        width={compact ? 32 : 40}
        height={compact ? 32 : 40}
      />
      <span className={cn('font-display font-semibold tracking-tight', compact ? 'text-sm' : 'text-[15px]')}>
        PairDesk
      </span>
    </div>
  );
}
