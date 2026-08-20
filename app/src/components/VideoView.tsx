/**
 * 远程画面显示组件。
 *
 * 订阅桥接层的 screen-frame 事件，把 JPEG 字节渲染成 <img>：
 *  - 用 Blob URL 让浏览器原生解码 JPEG（无额外解码成本）
 *  - 每次新帧替换 objectURL，释放旧 URL 防内存泄漏
 *  - 等比缩放填满容器，保持远端纵横比
 */
import { useEffect, useRef, useState } from 'react';
import { getCoreBridge } from '../bridge';
import { IconMonitor } from './icons';

interface VideoViewProps {
  aspect?: { w: number; h: number };
}

export function VideoView({ aspect }: VideoViewProps) {
  const [src, setSrc] = useState<string | null>(null);
  const urlRef = useRef<string | null>(null);

  useEffect(() => {
    return getCoreBridge().onEvent((e) => {
      if (e.type !== 'frame') return;
      const bytes = new Uint8Array(e.jpeg);
      const blob = new Blob([bytes], { type: 'image/jpeg' });
      const url = URL.createObjectURL(blob);
      if (urlRef.current) URL.revokeObjectURL(urlRef.current);
      urlRef.current = url;
      setSrc(url);
    });
  }, []);

  useEffect(() => {
    return () => {
      if (urlRef.current) URL.revokeObjectURL(urlRef.current);
    };
  }, []);

  return (
    <div
      className="pd-video flex min-h-[280px] flex-1 items-center justify-center overflow-hidden rounded-pd-lg border border-pd-border bg-[#07080a]"
      style={{ aspectRatio: aspect ? `${aspect.w} / ${aspect.h}` : undefined }}
    >
      {src ? (
        <img src={src} alt="远程画面" className="h-full w-full object-contain" />
      ) : (
        <span className="flex flex-col items-center gap-2 text-[13px] text-pd-muted">
          <IconMonitor size={28} className="opacity-50" />
          等待画面…
        </span>
      )}
    </div>
  );
}
