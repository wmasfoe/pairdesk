/**
 * 前端入口。
 *
 *  - Tailwind 先加载（含 preflight）
 *  - ui-kit 控件样式后加载，覆盖 preflight 对 button/input 的重置
 *  - 根类名 .pd-light / .pd-dark 跟随系统外观（见 watchSystemTheme）
 */
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import './App.css';
import '@pairdesk/ui-kit/styles.css';
import App from './App';
import { watchSystemTheme } from './theme';

watchSystemTheme();

const root = createRoot(document.getElementById('root')!);
root.render(
  <StrictMode>
    <div className="pd-app relative min-h-full bg-pd-bg font-sans text-[15px] leading-normal text-pd-fg antialiased">
      <div className="pd-glow pointer-events-none fixed inset-0" />
      <div className="relative z-10 min-h-full">
        <App />
      </div>
    </div>
  </StrictMode>,
);
