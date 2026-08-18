/**
 * 前端入口。
 *
 *  - 引入 ui-kit 全局样式（主题 CSS 变量）
 *  - 根容器挂 .pd-app .pd-light（亮色主题，系统深色跟随留待改进）
 */
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import '@pairdesk/ui-kit/styles.css';
import App from './App';
import './App.css';

const root = createRoot(document.getElementById('root')!);
root.render(
  <StrictMode>
    <div className="pd-app pd-light">
      <App />
    </div>
  </StrictMode>,
);
