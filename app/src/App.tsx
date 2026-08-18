/**
 * App 根组件：模式选择 + 页面切换。
 *
 * 职责单一：维护当前所处"模式"（无 / 被控 / 控制），渲染对应页面。
 * 模式切换用简单 state（首版单用户单界面，不引入路由库）。
 */
import { useState } from 'react';
import { Card } from '@pairdesk/ui-kit';
import { HostPage } from './pages/HostPage';
import { ViewerPage } from './pages/ViewerPage';

type Mode = null | 'host' | 'viewer';

export default function App() {
  const [mode, setMode] = useState<Mode>(null);

  if (mode === 'host') return <HostPage onBack={() => setMode(null)} />;
  if (mode === 'viewer') return <ViewerPage onBack={() => setMode(null)} />;

  // 模式选择：两个大卡片，让用户一进来就明确"我要做什么"
  return (
    <div className="pd-home">
      <h1 className="pd-home__title">PairDesk</h1>
      <p className="pd-home__sub">两人远程协助 · 选择你的角色</p>
      <div className="pd-home__modes">
        <button className="pd-modecard" onClick={() => setMode('host')}>
          <span className="pd-modecard__icon">🖥️</span>
          <span className="pd-modecard__title">被控端</span>
          <span className="pd-modecard__desc">让别人连接并操作我的电脑</span>
        </button>
        <button className="pd-modecard" onClick={() => setMode('viewer')}>
          <span className="pd-modecard__icon">🔭</span>
          <span className="pd-modecard__title">控制端</span>
          <span className="pd-modecard__desc">连接并操作另一台电脑</span>
        </button>
      </div>
    </div>
  );
}
