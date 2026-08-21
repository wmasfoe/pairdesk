import { useState } from 'react';
import { Button, Card } from '@pairdesk/ui-kit';
import { getCoreBridge } from '../bridge';
import type { UpdateInfo, UpdateProgress } from '../bridge/types';

interface UpdateModalProps {
  info: UpdateInfo;
  onClose: () => void;
}

export function UpdateModal({ info, onClose }: UpdateModalProps) {
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState<UpdateProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleInstall = async () => {
    setDownloading(true);
    setError(null);
    try {
      await getCoreBridge().installUpdate(info.downloadUrl, (p) => {
        setProgress(p);
      });
    } catch (e: any) {
      setDownloading(false);
      setError(e?.message ?? String(e));
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm">
      <Card className="w-full max-w-md border border-pd-border bg-pd-elev p-6 shadow-2xl text-left">
        <div className="flex items-center justify-between border-b border-pd-border/60 pb-3">
          <div className="flex items-center gap-2">
            <span className="text-xl">🚀</span>
            <h3 className="m-0 font-display text-base font-semibold text-pd-fg">发现新版本</h3>
          </div>
          <span className="rounded-full bg-pd-primary-soft px-2.5 py-0.5 text-[12px] font-medium text-pd-primary">
            v{info.latestVersion}
          </span>
        </div>

        <div className="mt-4 flex flex-col gap-2 text-[13px] text-pd-muted">
          <p className="m-0">
            当前版本: <span className="font-mono text-pd-fg">v{info.currentVersion}</span>
          </p>
          {info.releaseNotes && (
            <div className="mt-2 max-h-40 overflow-y-auto rounded-pd bg-black/20 p-3 text-[12px] leading-relaxed text-pd-fg/90 whitespace-pre-wrap border border-pd-border/40">
              {info.releaseNotes}
            </div>
          )}
        </div>

        {downloading && (
          <div className="mt-5 flex flex-col gap-2">
            <div className="flex justify-between text-[12px] text-pd-muted">
              <span>{progress?.percent === 100 ? '下载完成，正在自动安装并重启…' : '正在下载新版本…'}</span>
              <span className="font-mono font-medium text-pd-primary">{progress?.percent ?? 0}%</span>
            </div>
            <div className="h-2 w-full overflow-hidden rounded-full bg-pd-border/60">
              <div
                className="h-full bg-pd-primary transition-all duration-150"
                style={{ width: `${progress?.percent ?? 0}%` }}
              />
            </div>
          </div>
        )}

        {error && (
          <div className="mt-4 rounded-pd border border-pd-danger/25 bg-pd-danger/10 px-3 py-2 text-[12px] text-pd-danger">
            更新失败: {error}
          </div>
        )}

        <div className="mt-6 flex items-center justify-end gap-3">
          {!downloading && (
            <Button variant="ghost" size="sm" onClick={onClose}>
              稍后再说
            </Button>
          )}
          <Button
            variant="primary"
            size="sm"
            disabled={downloading}
            onClick={handleInstall}
          >
            {downloading ? '更新中…' : '应用内更新并重启'}
          </Button>
        </div>
      </Card>
    </div>
  );
}
