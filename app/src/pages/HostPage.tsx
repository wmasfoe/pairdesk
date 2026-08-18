/**
 * 被控端页：让别人连我。
 *
 * 展示连接信息（端口/密码），提供"开始接收协助"与"断开"操作。
 * 本地状态：端口、密码、是否启动。会话状态来自 useSession。
 */
import { Button, StatusDot, TextField } from '@pairdesk/ui-kit';
import { useState } from 'react';
import { useSession, type SessionPhase } from '../state/useSession';
import type { StatusTone } from '@pairdesk/ui-kit';

/** 会话阶段 → 状态点色调 */
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

export function HostPage({ onBack }: { onBack: () => void }) {
  const session = useSession();
  const [port, setPort] = useState('8888');
  const [password, setPassword] = useState(() => genPwd());
  const active = session.phase === 'connected' || session.phase === 'authenticated';

  return (
    <div className="pd-page pd-page--host">
      <header className="pd-page__head">
        <Button variant="ghost" size="sm" onClick={onBack}>← 返回</Button>
        <h1 className="pd-page__title">被控端（让别人连我）</h1>
        <StatusDot tone={TONE[session.phase]} pulse={session.phase === 'authentication'}>
          {session.phase === 'connected' ? '有人正在观看你的屏幕' : '等待连接'}
        </StatusDot>
      </header>

      {!active ? (
        <div className="pd-host-form">
          <TextField
            label="监听端口"
            value={port}
            onChange={setPort}
            placeholder="8888"
            inputMode="numeric"
          />
          {/* 密码：每次启动可重新生成，展示为纯数字便于口口相传 */}
          <div className="pd-password">
            <TextField label="连接密码" value={password} onChange={setPassword} />
            <Button variant="ghost" onClick={() => setPassword(genPwd())}>换一个</Button>
          </div>
          <p className="pd-hint">
            把「本机 IP + 端口 + 密码」告诉对方即可。示例 IP：
            <code>127.0.0.1:{port}</code>
          </p>
          <Button
            variant="primary"
            onClick={() => session.startHost(Number(port) || 8888, password)}
          >
            开始接收协助
          </Button>
          {session.error && <p className="pd-error">{session.error}</p>}
        </div>
      ) : (
        <div className="pd-host-active">
          <h2>正在等待连接</h2>
          <p className="pd-bigpwd">{password}</p>
          <p className="pd-hint">对方在你的设备列表或输入框里看到本机，输入此密码即可连接。</p>
          <Button variant="danger" onClick={session.disconnect}>断开连接</Button>
        </div>
      )}
    </div>
  );
}
