/**
 * App 根组件：模式选择 + 页面切换。
 *
 * 职责单一：维护当前所处"模式"（无 / 被控 / 控制），渲染对应页面。
 * 模式切换用简单 state（首版单用户单界面，不引入路由库）。
 */
import { useState } from 'react';
import { HostPage } from './pages/HostPage';
import { ViewerPage } from './pages/ViewerPage';
import { usePermissions } from './state/usePermissions';
import { PermissionBanner } from './components/PermissionBanner';
import { Brand } from './components/Brand';
import { IconMonitor, IconPointer } from './components/icons';

type Mode = null | 'host' | 'viewer';

export default function App() {
  const [mode, setMode] = useState<Mode>(null);
  const perms = usePermissions();

  if (mode === 'host') return <HostPage onBack={() => setMode(null)} />;
  if (mode === 'viewer') return <ViewerPage onBack={() => setMode(null)} />;

  return (
    <div className="flex min-h-full flex-col items-center justify-center px-6 py-12 text-center">
      <Brand />
      <h1 className="mt-8 font-display text-[1.75rem] font-medium tracking-tight text-pd-fg">
        两台电脑，一次远程协助
      </h1>
      <p className="mt-2 text-sm text-pd-muted">同网直连 · 跨网打洞 · 选择你的角色</p>

      <div className="mt-8 w-full max-w-[560px] text-left">
        <PermissionBanner
          needGuidance={perms.needGuidance}
          screenRecording={perms.screenRecording}
          accessibility={perms.accessibility}
          requiredFor="both"
          onRequest={perms.request}
          onOpenSettings={perms.openSettings}
          onRecheck={perms.recheck}
        />
      </div>

      <div className="mt-10 flex w-full max-w-[560px] flex-wrap justify-center gap-4">
        <button className="pd-modecard group flex max-w-[270px] flex-1 basis-[220px] flex-col items-start rounded-pd-lg border border-pd-border bg-pd-elev p-6 text-left transition duration-150 hover:-translate-y-0.5 hover:border-pd-primary/50 hover:shadow-[0_12px_32px_rgba(0,0,0,0.22)] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-pd-primary" onClick={() => setMode('host')}>
          <span className="mb-3 flex size-10 items-center justify-center rounded-pd bg-pd-primary-soft text-pd-primary">
            <IconMonitor size={20} />
          </span>
          <span className="font-display text-base font-semibold tracking-tight text-pd-fg">被控端</span>
          <span className="mt-1 text-[13px] leading-snug text-pd-muted">分享屏幕，让对方操作这台电脑</span>
        </button>
        <button className="pd-modecard group flex max-w-[270px] flex-1 basis-[220px] flex-col items-start rounded-pd-lg border border-pd-border bg-pd-elev p-6 text-left transition duration-150 hover:-translate-y-0.5 hover:border-pd-primary/50 hover:shadow-[0_12px_32px_rgba(0,0,0,0.22)] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-pd-primary" onClick={() => setMode('viewer')}>
          <span className="mb-3 flex size-10 items-center justify-center rounded-pd bg-pd-primary-soft text-pd-primary">
            <IconPointer size={20} />
          </span>
          <span className="font-display text-base font-semibold tracking-tight text-pd-fg">控制端</span>
          <span className="mt-1 text-[13px] leading-snug text-pd-muted">输入会话码，远程操作另一台电脑</span>
        </button>
      </div>
    </div>
  );
}
