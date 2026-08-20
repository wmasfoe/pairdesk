/**
 * 前端入口。
 *
 *  - Tailwind 先加载（含 preflight）
 *  - ui-kit 控件样式后加载，覆盖 preflight 对 button/input 的重置
 *  - 根容器挂 .pd-dark，供 ui-kit CSS 变量取色
 */
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import './App.css';
import '@pairdesk/ui-kit/styles.css';
import App from './App';

const root = createRoot(document.getElementById('root')!);
root.render(
  <StrictMode>
    <div className="pd-app pd-dark relative min-h-full bg-pd-bg font-sans text-[15px] leading-normal text-pd-fg antialiased">
      <div className="pd-glow pointer-events-none fixed inset-0" />
      <div className="relative z-10 min-h-full">
        <App />
      </div>
    </div>
  </StrictMode>,
);
