/**
 * macOS 权限检测引导横幅。
 */
import { Button } from '@pairdesk/ui-kit';
import { IconKeyboard, IconScreen, IconShield } from './icons';

interface PermissionBannerProps {
  needGuidance: boolean;
  screenRecording: boolean;
  accessibility: boolean;
  requiredFor: 'host' | 'viewer' | 'both';
  onRequest: (type: 'screen' | 'accessibility') => void;
  onOpenSettings: (type: 'screen' | 'accessibility') => void;
  onRecheck: () => void;
}

export function PermissionBanner({
  needGuidance,
  screenRecording,
  accessibility,
  requiredFor,
  onRequest,
  onOpenSettings,
  onRecheck,
}: PermissionBannerProps) {
  if (!needGuidance) return null;

  const needScreen = (requiredFor === 'host' || requiredFor === 'both') && !screenRecording;
  const needAccessibility = (requiredFor === 'viewer' || requiredFor === 'both') && !accessibility;

  if (!needScreen && !needAccessibility) return null;

  return (
    <div className="flex flex-col gap-3 rounded-pd-lg border border-pd-warn/30 bg-pd-warn/10 px-4 py-4 text-left">
      <div className="flex items-start gap-3">
        <span className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-pd-warn/20 text-pd-warn">
          <IconShield size={16} />
        </span>
        <div>
          <h4 className="m-0 text-sm font-semibold text-pd-fg">需要授予系统权限</h4>
          <p className="mt-0.5 mb-0 text-xs leading-snug text-pd-muted">
            macOS 安全机制要求明确授权后才能进行屏幕查看与输入操作。
          </p>
        </div>
      </div>

      <div className="flex flex-col gap-2">
        {needScreen && (
          <div className="flex flex-wrap items-center justify-between gap-3 rounded-pd border border-pd-border bg-pd-bg px-3 py-2">
            <div className="flex min-w-0 flex-wrap items-center gap-2">
              <span className="inline-flex items-center gap-1.5 text-[13px] font-medium">
                <IconScreen size={14} /> 屏幕录制
              </span>
              <span className="rounded bg-pd-warn/20 px-1.5 py-0.5 text-[11px] text-pd-warn">未授权 · 被控端无法传画面</span>
            </div>
            <div className="flex items-center gap-1.5">
              <Button size="sm" variant="secondary" onClick={() => onRequest('screen')}>
                请求授权
              </Button>
              <Button size="sm" variant="ghost" onClick={() => onOpenSettings('screen')}>
                打开设置
              </Button>
            </div>
          </div>
        )}

        {needAccessibility && (
          <div className="flex flex-wrap items-center justify-between gap-3 rounded-pd border border-pd-border bg-pd-bg px-3 py-2">
            <div className="flex min-w-0 flex-wrap items-center gap-2">
              <span className="inline-flex items-center gap-1.5 text-[13px] font-medium">
                <IconKeyboard size={14} /> 辅助功能
              </span>
              <span className="rounded bg-pd-warn/20 px-1.5 py-0.5 text-[11px] text-pd-warn">未授权 · 控制端无法输入</span>
            </div>
            <div className="flex items-center gap-1.5">
              <Button size="sm" variant="ghost" onClick={() => onOpenSettings('accessibility')}>
                去设置勾选
              </Button>
            </div>
          </div>
        )}
      </div>

      <div className="flex flex-wrap items-center justify-between gap-3 border-t border-pd-border pt-2.5">
        <span className="text-[13px] text-pd-muted">授权后切回 App 会自动刷新</span>
        <Button size="sm" variant="ghost" onClick={onRecheck}>
          已开启，刷新检测
        </Button>
      </div>
    </div>
  );
}
