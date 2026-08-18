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

interface VideoViewProps {
  /** 远端分辨率（用于占位比例 box） */
  aspect?: { w: number; h: number };
}

export function VideoView({ aspect }: VideoViewProps) {
  const [src, setSrc] = useState<string | null>(null);
  const urlRef = useRef<string | null>(null);

  useEffect(() => {
    // 订阅画面帧；返回取消订阅函数（组件卸载时自动移除）
    return getCoreBridge().onEvent((e) => {
      if (e.type !== 'frame') return;
      // 拷贝为 ArrayBuffer 底板（规避 TS 5.7+ Uint8Array<ArrayBufferLike> 泛型歧义）
      const bytes = new Uint8Array(e.jpeg);
      const blob = new Blob([bytes], { type: 'image/jpeg' });
      const url = URL.createObjectURL(blob);
      if (urlRef.current) URL.revokeObjectURL(urlRef.current);
      urlRef.current = url;
      setSrc(url);
    });
  }, []);

  // 卸载时释放最后一个 URL
  useEffect(() => {
    return () => {
      if (urlRef.current) URL.revokeObjectURL(urlRef.current);
    };
  }, []);

  return (
    <div
      className="pd-video"
      style={{
        aspectRatio: aspect ? `${aspect.w} / ${aspect.h}` : undefined,
        background: '#0f172a',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        overflow: 'hidden',
      }}
    >
      {src ? (
        <img src={src} alt="远程画面" style={{ width: '100%', height: '100%', objectFit: 'contain' }} />
      ) : (
        <span style={{ color: '#94a3b8' }}>等待画面…</span>
      )}
    </div>
  );
}
