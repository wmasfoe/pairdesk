/**
 * 控制端页：我去连别人。
 *
 * 输入对方给的【会话码 + 密码】（中继地址可改），程序自动择一连接。
 */
import { Button, Card, StatusDot, TextField } from '@pairdesk/ui-kit';
import { useState } from 'react';
import { useSession, type SessionPhase } from '../state/useSession';
import { usePermissions } from '../state/usePermissions';
import { PermissionBanner } from '../components/PermissionBanner';
import { VideoView } from '../components/VideoView';
import { PageHeader } from '../components/PageHeader';
import { AdvancedPanel } from '../components/AdvancedPanel';
import type { StatusTone } from '@pairdesk/ui-kit';
import { DEFAULT_RELAY } from '../constants';
import { cn } from '../lib/cn';

const TONE: Record<SessionPhase, StatusTone> = {
  idle: 'idle',
  authentication: 'connecting',
  authenticated: 'online',
  connected: 'online',
  disconnected: 'idle',
  error: 'error',
};

export function ViewerPage({ onBack }: { onBack: () => void }) {
  const session = useSession();
  const perms = usePermissions();
  const [relay, setRelay] = useState(DEFAULT_RELAY);
  const [sid, setSid] = useState('');
  const [password, setPassword] = useState('');
  const connected = session.phase === 'connected' || session.phase === 'authenticated';
  const busy = session.phase === 'authentication';

  return (
    <div className={cn('flex min-h-full flex-col gap-5', connected ? 'h-full gap-3 px-3.5 py-3' : 'mx-auto max-w-lg px-6 py-6')}>
      <PageHeader title="控制端" onBack={onBack}>
        <StatusDot tone={TONE[session.phase]} pulse={busy}>
          {labelOf(session.phase)}
        </StatusDot>
      </PageHeader>

      <PermissionBanner
        needGuidance={perms.needGuidance}
        screenRecording={perms.screenRecording}
        accessibility={perms.accessibility}
        requiredFor="viewer"
        onRequest={perms.request}
        onOpenSettings={perms.openSettings}
        onRestart={perms.restart}
      />

      {!connected ? (
        <div className="flex flex-col gap-4">
          <p className="text-[13px] text-pd-muted">输入对方给的会话码和密码，程序会自动选择最优链路。</p>
          <Card className="flex flex-col gap-4">
            <TextField label="对方会话码" value={sid} onChange={setSid} placeholder="如 pd-AB12C34" autoComplete="off" spellCheck={false} />
            <TextField label="连接密码" value={password} onChange={setPassword} placeholder="一次性密码" type="password" />
          </Card>
          <Button
            variant="primary"
            className="pd-btn--block"
            size="lg"
            disabled={!sid || !password || busy}
            onClick={() => session.connectAuto(relay, sid, password)}
          >
            {busy ? '连接中…' : '连接'}
          </Button>
          <AdvancedPanel>
            <TextField label="中继/VPS 地址" value={relay} onChange={setRelay} placeholder={DEFAULT_RELAY} />
          </AdvancedPanel>
          {session.transport && <p className="m-0 text-[13px] text-pd-muted">传输路径：{session.transport}</p>}
          {session.error && (
            <p className="m-0 rounded-pd border border-pd-danger/25 bg-pd-danger/10 px-3 py-2 text-[13px] text-pd-danger">
              {session.error}
            </p>
          )}
        </div>
      ) : (
        <div className="flex min-h-0 flex-1 flex-col gap-3">
          <div className="flex items-center gap-3 rounded-pd border border-pd-border bg-pd-elev py-2 pr-2.5 pl-3.5">
            <span className="flex-1 font-mono text-sm font-semibold tracking-wide text-pd-fg">{sid}</span>
            <span className="text-[13px] text-pd-muted">{session.transport ?? ''}</span>
            <Button variant="danger" size="sm" onClick={session.disconnect}>断开</Button>
          </div>
          <VideoView aspect={session.screen ?? undefined} />
        </div>
      )}
    </div>
  );
}

function labelOf(p: SessionPhase): string {
  switch (p) {
    case 'idle': return '未连接';
    case 'authentication': return '连接中…';
    case 'authenticated': return '已认证，等待画面';
    case 'connected': return '已连接';
    case 'disconnected': return '已断开';
    case 'error': return '出错';
  }
}
