/**
 * 被控端页：让别人连我。
 *
 * 用户模型：本机生成【会话码 + 密码】，打开「允许远程控制」开关后开始接收协助。
 * 中继地址用于打洞信令与兜底（生产为 VPS，可改）。
 */
import { Button, Card, StatusDot, TextField } from '@pairdesk/ui-kit';
import { useState } from 'react';
import { getCoreBridge } from '../bridge';
import { useSession, type SessionPhase } from '../state/useSession';
import { usePermissions } from '../state/usePermissions';
import { PermissionBanner } from '../components/PermissionBanner';
import { Credential } from '../components/Credential';
import { PageHeader } from '../components/PageHeader';
import { AllowToggle } from '../components/AllowToggle';
import { AdvancedPanel } from '../components/AdvancedPanel';
import { IconCheck, IconCopy } from '../components/icons';
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

function genPwd(): string {
  return Math.floor(100000 + Math.random() * 900000).toString();
}
function genSid(): string {
  return 'pd-' + Math.random().toString(36).slice(2, 6).toUpperCase() + Math.floor(100 + Math.random() * 900);
}

function statusLabel(phase: SessionPhase): string {
  if (phase === 'connected') return '有人正在观看';
  if (phase === 'authentication' || phase === 'authenticated') return '等待连接';
  if (phase === 'error') return '出错';
  return '未开始';
}

export function HostPage({ onBack }: { onBack: () => void }) {
  const session = useSession();
  const perms = usePermissions();
  const [relay, setRelay] = useState(DEFAULT_RELAY);
  const [sid, setSid] = useState(() => genSid());
  const [holePort, setHolePort] = useState(DEFAULT_HOLE_PORT);
  const [password, setPassword] = useState(() => genPwd());
  const [allowed, setAllowed] = useState(false);
  const [copiedAll, setCopiedAll] = useState(false);
  const active = session.phase === 'connected' || session.phase === 'authenticated' || session.phase === 'authentication';

  const toggleAllowed = (v: boolean) => {
    setAllowed(v);
    void getCoreBridge().setAllowed(v);
  };

  const copyShare = async () => {
    try {
      await navigator.clipboard.writeText(`会话码：${sid}\n密码：${password}`);
      setCopiedAll(true);
      window.setTimeout(() => setCopiedAll(false), 1400);
    } catch {
      /* ignore */
    }
  };

  return (
    <div className="mx-auto flex min-h-full max-w-lg flex-col gap-5 px-6 py-6">
      <PageHeader title="被控端" onBack={onBack}>
        <StatusDot tone={TONE[session.phase]} pulse={session.phase === 'authentication' || session.phase === 'authenticated'}>
          {statusLabel(session.phase)}
        </StatusDot>
      </PageHeader>

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
        <div className="flex flex-col gap-4">
          <p className="text-[13px] text-pd-muted">把会话码和密码发给对方，打开开关后即可等待连入。</p>
          <Card className="flex flex-col gap-4">
            <Credential label="会话码" value={sid} onChange={setSid} onRefresh={() => setSid(genSid())} large />
            <Credential label="连接密码" value={password} onChange={setPassword} onRefresh={() => setPassword(genPwd())} large />
          </Card>
          <AllowToggle checked={allowed} onChange={toggleAllowed} />
          <Button
            variant="primary"
            className="pd-btn--block"
            size="lg"
            disabled={!allowed || !sid || !password}
            onClick={() => session.startHostAuto(relay, sid, Number(holePort) || 23517, password)}
          >
            开始接收协助
          </Button>
          <AdvancedPanel>
            <TextField label="中继/VPS 地址" value={relay} onChange={setRelay} placeholder={DEFAULT_RELAY} />
            <TextField label="打洞端口" value={holePort} onChange={setHolePort} inputMode="numeric" />
          </AdvancedPanel>
          {session.notice && <p className="m-0 rounded-pd bg-pd-primary-soft px-3 py-2 text-[13px] text-pd-primary">{session.notice}</p>}
          {session.error && (
            <p className="m-0 rounded-pd border border-pd-danger/25 bg-pd-danger/10 px-3 py-2 text-[13px] text-pd-danger">
              {session.error}
            </p>
          )}
        </div>
      ) : (
        <div className="flex flex-col items-center gap-3 pt-4 text-center">
          <Card className="flex w-full flex-col items-center gap-5 px-5 py-6 text-center">
            <p className="m-0 text-[11px] font-medium uppercase tracking-[0.08em] text-pd-muted">
              {session.phase === 'connected' ? '正在被远程控制' : '正在等待对方连接'}
            </p>
            <div className="flex flex-col gap-1">
              <p className="m-0 font-mono text-[1.75rem] font-semibold tracking-[0.14em] text-pd-fg tabular-nums">{sid}</p>
              <p className="m-0 text-[13px] text-pd-muted">会话码</p>
            </div>
            <div className="flex flex-col gap-1">
              <p className="m-0 font-mono text-[1.375rem] font-semibold tracking-[0.28em] text-pd-primary tabular-nums">{password}</p>
              <p className="m-0 text-[13px] text-pd-muted">连接密码</p>
            </div>
            <div className="flex w-full flex-col gap-2">
              <Button variant="secondary" className="pd-btn--block" onClick={copyShare}>
                {copiedAll ? <IconCheck size={16} /> : <IconCopy size={16} />}
                {copiedAll ? '已复制' : '复制会话信息'}
              </Button>
              <Button variant="danger" className="pd-btn--block" onClick={session.disconnect}>
                断开连接
              </Button>
            </div>
          </Card>
          {session.transport && <p className="m-0 text-[13px] text-pd-muted">传输路径：{session.transport}</p>}
          {session.notice && <p className="m-0 rounded-pd bg-pd-primary-soft px-3 py-2 text-[13px] text-pd-primary">{session.notice}</p>}
        </div>
      )}
    </div>
  );
}
