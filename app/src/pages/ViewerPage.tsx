/**
 * 控制端页：我去连别人。
 *
 * 输入对方给的【会话码 + 密码】（中继地址可改），程序自动择一连接。
 */
import { Button, StatusDot, TextField } from '@pairdesk/ui-kit';
import { useState } from 'react';
import { useSession, type SessionPhase } from '../state/useSession';
import { VideoView } from '../components/VideoView';
import type { StatusTone } from '@pairdesk/ui-kit';
import { DEFAULT_RELAY } from '../constants';

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
  const [relay, setRelay] = useState(DEFAULT_RELAY);
  const [sid, setSid] = useState('');
  const [password, setPassword] = useState('');
  const connected = session.phase === 'connected' || session.phase === 'authenticated';
  const busy = session.phase === 'authentication';

  return (
    <div className="pd-page pd-page--viewer">
      <header className="pd-page__head">
        <Button variant="ghost" size="sm" onClick={onBack}>← 返回</Button>
        <h1 className="pd-page__title">控制端（去连别人）</h1>
        <StatusDot tone={TONE[session.phase]} pulse={busy}>
          {labelOf(session.phase)}
        </StatusDot>
      </header>

      {!connected ? (
        <div className="pd-viewer-form">
          <TextField label="对方会话码" value={sid} onChange={setSid} placeholder="如 pd-AB12C34" />
          <TextField label="连接密码" value={password} onChange={setPassword} placeholder="一次性密码" type="password" />
          <details className="pd-advanced">
            <summary>高级设置</summary>
            <div className="pd-advanced__body">
              <TextField label="中继/VPS 地址" value={relay} onChange={setRelay} placeholder={DEFAULT_RELAY} />
            </div>
          </details>
          <Button variant="primary" disabled={!sid || !password} onClick={() => session.connectAuto(relay, sid, password)}>
            {busy ? '连接中…' : '连接'}
          </Button>
          {session.transport && <p className="pd-hint">传输路径：{session.transport}</p>}
          {session.error && <p className="pd-error">{session.error}</p>}
        </div>
      ) : (
        <div className="pd-viewer-active">
          <div className="pd-toolbar">
            <span className="pd-toolbar__addr">{sid}</span>
            <span className="pd-hint">{session.transport ?? ''}</span>
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
