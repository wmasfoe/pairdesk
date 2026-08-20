/**
 * macOS / 跨平台权限检测与引导 Hook。
 *
 * 在 App 打开或进入被控端/控制端时检测系统权限：
 * - 屏幕录制（被控端抓屏需要）
 * - 辅助功能（控制端注入鼠标/键盘需要）
 */
import { useCallback, useEffect, useState } from 'react';
import { getCoreBridge } from '../bridge';
import type { PermissionStatus } from '../bridge/types';

export function usePermissions() {
  const [status, setStatus] = useState<PermissionStatus>({
    screenRecording: true,
    accessibility: true,
    needGuidance: false,
  });
  const [loading, setLoading] = useState(true);

  const check = useCallback(async () => {
    try {
      const res = await getCoreBridge().checkPermissions();
      setStatus(res);
    } catch {
      // 非原生环境或出错时默认不阻塞
      setStatus({ screenRecording: true, accessibility: true, needGuidance: false });
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void check();

    // 当窗口重新获得焦点（如用户从系统设置授权回来后），自动重新检测权限
    const onFocus = () => {
      void check();
    };
    window.addEventListener('focus', onFocus);
    return () => {
      window.removeEventListener('focus', onFocus);
    };
  }, [check]);

  const request = async (type: 'screen' | 'accessibility') => {
    await getCoreBridge().requestPermission(type);
    // 稍后重新检测
    setTimeout(() => {
      void check();
    }, 1000);
  };

  const openSettings = async (type: 'screen' | 'accessibility') => {
    await getCoreBridge().openPermissionSettings(type);
  };

  return {
    ...status,
    loading,
    recheck: check,
    request,
    openSettings,
  };
}
