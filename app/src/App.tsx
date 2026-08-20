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
import { usePermissions } from './state/usePermissions';
import { PermissionBanner } from './components/PermissionBanner';

type Mode = null | 'host' | 'viewer';

export default function App() {
  const [mode, setMode] = useState<Mode>(null);
  const perms = usePermissions();

  if (mode === 'host') return <HostPage onBack={() => setMode(null)} />;
  if (mode === 'viewer') return <ViewerPage onBack={() => setMode(null)} />;

  // 模式选择：一句话引导 + 两个大卡片（方向 C 助手式）
  return (
    <div className="pd-home">
      <p className="pd-home__brand">🐈 PairDesk</p>
      <h1 className="pd-home__title">让 <b>好友</b> 帮你<br />远程处理</h1>
      <p className="pd-home__sub">一台当被控端、一台当控制端 · 选择你的角色</p>

      <PermissionBanner
        needGuidance={perms.needGuidance}
        screenRecording={perms.screenRecording}
        accessibility={perms.accessibility}
        requiredFor="both"
        onRequest={perms.request}
        onOpenSettings={perms.openSettings}
        onRecheck={perms.recheck}
      />

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
