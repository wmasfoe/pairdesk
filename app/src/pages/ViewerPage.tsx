/**
 * 控制端页：我去连别人。
 *
 * 提供地址/密码输入 → 连接 → 显示远程画面 + 工具栏（断开）。
 */
import { Button, StatusDot, TextField } from '@pairdesk/ui-kit';
import { useState } from 'react';
import { useSession, type SessionPhase } from '../state/useSession';
import { VideoView } from '../components/VideoView';
import type { StatusTone } from '@pairdesk/ui-kit';

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
  const [addr, setAddr] = useState('127.0.0.1:8888');
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
          <TextField label="对方地址 (IP:端口)" value={addr} onChange={setAddr} placeholder="192.168.1.10:8888" />
          <TextField label="连接密码" value={password} onChange={setPassword} placeholder="一次性密码" type="password" />
          <Button variant="primary" disabled={!addr || !password} onClick={() => session.connect(addr, password)}>
            {busy ? '连接中…' : '连接'}
          </Button>
          {session.error && <p className="pd-error">{session.error}</p>}
        </div>
      ) : (
        <div className="pd-viewer-active">
          <div className="pd-toolbar">
            <span className="pd-toolbar__addr">{addr}</span>
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
