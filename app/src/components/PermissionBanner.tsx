/**
 * macOS 权限检测引导横幅 / 弹窗组件。
 */
import { Button } from '@pairdesk/ui-kit';

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
    <div className="pd-perm-banner">
      <div className="pd-perm-banner__head">
        <span className="pd-perm-banner__icon">⚠️</span>
        <div>
          <h4 className="pd-perm-banner__title">需要授予系统权限</h4>
          <p className="pd-perm-banner__desc">
            macOS 安全机制要求明确授权后才能正常进行屏幕查看与输入操作。
          </p>
        </div>
      </div>

      <div className="pd-perm-banner__list">
        {needScreen && (
          <div className="pd-perm-item">
            <div className="pd-perm-item__info">
              <span className="pd-perm-item__name">🖥️ 屏幕录制权限</span>
              <span className="pd-perm-item__tag pd-perm-item__tag--warn">未授权（被控端无法传画面）</span>
            </div>
            <div className="pd-perm-item__actions">
              <Button size="sm" variant="secondary" onClick={() => onRequest('screen')}>
                请求授权
              </Button>
              <Button size="sm" variant="ghost" onClick={() => onOpenSettings('screen')}>
                打开设置 ↗
              </Button>
            </div>
          </div>
        )}

        {needAccessibility && (
          <div className="pd-perm-item">
            <div className="pd-perm-item__info">
              <span className="pd-perm-item__name">⌨️ 辅助功能权限</span>
              <span className="pd-perm-item__tag pd-perm-item__tag--warn">未授权（控制端无法输入控制）</span>
            </div>
            <div className="pd-perm-item__actions">
              <Button size="sm" variant="secondary" onClick={() => onOpenSettings('accessibility')}>
                去设置勾选 ↗
              </Button>
            </div>
          </div>
        )}
      </div>

      <div className="pd-perm-banner__footer">
        <span className="pd-hint">授权后切换回 App 会自动刷新状态</span>
        <Button size="sm" variant="ghost" onClick={onRecheck}>
          已开启，刷新检测
        </Button>
      </div>
    </div>
  );
}
