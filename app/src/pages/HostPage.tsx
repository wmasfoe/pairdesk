/**
 * 被控端页：让别人连我。
 *
 * 用户模型：本机生成【会话码 + 密码】，打开「允许远程控制」开关后开始接收协助。
 * 中继地址用于打洞信令与兜底（生产为 VPS，可改）。
 */
import { Button, StatusDot, TextField } from '@pairdesk/ui-kit';
import { useState } from 'react';
import { getCoreBridge } from '../bridge';
import { useSession, type SessionPhase } from '../state/useSession';
import { usePermissions } from '../state/usePermissions';
import { PermissionBanner } from '../components/PermissionBanner';
import type { StatusTone } from '@pairdesk/ui-kit';
import { DEFAULT_HOLE_PORT, DEFAULT_RELAY } from '../constants';

const TONE: Record<SessionPhase, StatusTone> = {
  idle: 'idle',
  authentication: 'connecting',
  authenticated: 'online',
  connected: 'online',
  disconnected: 'idle',
  error: 'error',
};

/** 生成 6 位数字一次性密码 */
function genPwd(): string {
  return Math.floor(100000 + Math.random() * 900000).toString();
}
/** 生成会话码（如 pd-XXXX） */
function genSid(): string {
  return 'pd-' + Math.random().toString(36).slice(2, 6).toUpperCase() + Math.floor(100 + Math.random() * 900);
}

export function HostPage({ onBack }: { onBack: () => void }) {
  const session = useSession();
  const perms = usePermissions();
  const [relay, setRelay] = useState(DEFAULT_RELAY);
  const [sid, setSid] = useState(() => genSid());
  const [holePort, setHolePort] = useState(DEFAULT_HOLE_PORT);
  const [password, setPassword] = useState(() => genPwd());
  const [allowed, setAllowed] = useState(false);
  const active = session.phase === 'connected' || session.phase === 'authenticated';

  const toggleAllowed = (v: boolean) => {
    setAllowed(v);
    void getCoreBridge().setAllowed(v);
  };

  return (
    <div className="pd-page pd-page--host">
      <header className="pd-page__head">
        <Button variant="ghost" size="sm" onClick={onBack}>← 返回</Button>
        <h1 className="pd-page__title">被控端（让别人连我）</h1>
        <StatusDot tone={TONE[session.phase]} pulse={session.phase === 'authentication'}>
          {session.phase === 'connected' ? '有人正在观看你的屏幕' : '等待连接'}
        </StatusDot>
      </header>

      <PermissionBanner
        needGuidance={perms.needGuidance}
        screenRecording={perms.screenRecording}
        accessibility={perms.accessibility}
        requiredFor="host"
        onRequest={perms.request}
        onOpenSettings={perms.openSettings}
        onRecheck={perms.recheck}
      />

      {!active ? (
        <div className="pd-host-form">
          <div className="pd-field-row">
            <TextField label="会话码（给对方看）" value={sid} onChange={setSid} />
            <Button variant="ghost" onClick={() => setSid(genSid())}>换一个</Button>
          </div>
          <div className="pd-password">
            <TextField label="连接密码" value={password} onChange={setPassword} />
            <Button variant="ghost" onClick={() => setPassword(genPwd())}>换一个</Button>
          </div>
          <details className="pd-advanced">
            <summary>高级设置</summary>
            <div className="pd-advanced__body">
              <TextField label="中继/VPS 地址" value={relay} onChange={setRelay} placeholder={DEFAULT_RELAY} />
              <TextField label="打洞端口" value={holePort} onChange={setHolePort} inputMode="numeric" />
            </div>
          </details>
          <label className="pd-switch">
            <input type="checkbox" checked={allowed} onChange={(e) => toggleAllowed(e.target.checked)} />
            允许远程控制（关闭时即使有码也无法连接）
          </label>
          <Button
            variant="primary"
            disabled={!allowed || !sid || !password}
            onClick={() => session.startHostAuto(relay, sid, Number(holePort) || 23517, password)}
          >
            开始接收协助
          </Button>
          {session.notice && <p className="pd-notice">{session.notice}</p>}
          {session.error && <p className="pd-error">{session.error}</p>}
        </div>
      ) : (
        <div className="pd-host-active">
          <h2>正在等待连接</h2>
          <p className="pd-bigpwd">{sid}</p>
          <p className="pd-hint">把「会话码 {sid}」和密码 {password} 告诉对方，对方输入后即可看到你的屏幕。</p>
          <Button variant="danger" onClick={session.disconnect}>断开连接</Button>
        </div>
      )}
    </div>
  );
}
