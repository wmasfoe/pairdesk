/**
 * 远程画面显示与输入捕获组件。
 *
 * 1. 订阅桥接层的 screen-frame 事件，把 JPEG 字节渲染成 <img>
 * 2. 捕获鼠标（移动、点击、释放、右键、中键、滚轮）与键盘事件，
 *    按远端屏幕纵横比映射为绝对坐标 (x, y)，通过 bridge.sendInput 注入到远端。
 */
import { useEffect, useRef, useState, type MouseEvent, type WheelEvent, type KeyboardEvent } from 'react';
import { getCoreBridge } from '../bridge';
import { IconMonitor } from './icons';

interface VideoViewProps {
  aspect?: { w: number; h: number };
}

/** 将浏览器 KeyboardEvent.code / key 转换为 PairDesk 后端协议期望的 X11 keysym */
function keyToSym(code: string, key: string): number {
  // 1. 常见功能/导航键 (X11 keysym)
  switch (code) {
    case 'Backspace': return 0xff08;
    case 'Tab': return 0xff09;
    case 'Enter':
    case 'NumpadEnter': return 0xff0d;
    case 'Escape': return 0xff1b;
    case 'Delete': return 0xffff;
    case 'ArrowLeft': return 0xff51;
    case 'ArrowUp': return 0xff52;
    case 'ArrowRight': return 0xff53;
    case 'ArrowDown': return 0xff54;
    case 'Home': return 0xff50;
    case 'End': return 0xff57;
    case 'PageUp': return 0xff55;
    case 'PageDown': return 0xff56;
    case 'ShiftLeft':
    case 'ShiftRight': return 0xffe1;
    case 'ControlLeft':
    case 'ControlRight': return 0xffe3;
    case 'AltLeft':
    case 'AltRight': return 0xffe9;
    case 'MetaLeft':
    case 'MetaRight': return 0xffe7;
    case 'Space': return 0x20;
  }

  // 2. 单字符 ASCII (直接取 unicode 码点)
  if (key.length === 1) {
    const codePoint = key.charCodeAt(0);
    if (codePoint >= 32 && codePoint <= 126) {
      return codePoint;
    }
  }

  // 3. 其他非 ASCII 字符或 IME 组合中过程键，返回 0 忽略，避免产生脏键码破坏接收端
  return 0;
}

export function VideoView({ aspect }: VideoViewProps) {
  const [src, setSrc] = useState<string | null>(null);
  const urlRef = useRef<string | null>(null);
  const imgRef = useRef<HTMLImageElement | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);

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

  // 坐标换算：把控制端容器内的 clientX/clientY 映射到远端画面的真实分辨率 (x, y)
  const getRemoteCoord = (e: MouseEvent): { x: number; y: number } | null => {
    const img = imgRef.current;
    if (!img) return null;
    const rect = img.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return null;

    const relX = Math.max(0, Math.min(e.clientX - rect.left, rect.width));
    const relY = Math.max(0, Math.min(e.clientY - rect.top, rect.height));

    const targetW = aspect?.w ?? img.naturalWidth ?? rect.width;
    const targetH = aspect?.h ?? img.naturalHeight ?? rect.height;

    const x = (relX / rect.width) * targetW;
    const y = (relY / rect.height) * targetH;
    return { x, y };
  };

  const handleMouseMove = (e: MouseEvent) => {
    const coord = getRemoteCoord(e);
    if (!coord) return;
    getCoreBridge().sendInput({
      kind: 'mouse-move',
      x: coord.x,
      y: coord.y,
    });
  };

  const handleMouseDown = (e: MouseEvent) => {
    e.preventDefault();
    containerRef.current?.focus();
    const coord = getRemoteCoord(e);
    if (coord) {
      getCoreBridge().sendInput({
        kind: 'mouse-move',
        x: coord.x,
        y: coord.y,
      });
    }
    // e.button: 0=左键, 1=中键, 2=右键 -> PairDesk protocol: 1=左键, 2=中键, 3=右键
    const btn = e.button === 0 ? 1 : e.button === 1 ? 2 : e.button === 2 ? 3 : 1;
    getCoreBridge().sendInput({
      kind: 'button',
      btn,
      down: true,
    });
  };

  const handleMouseUp = (e: MouseEvent) => {
    e.preventDefault();
    const btn = e.button === 0 ? 1 : e.button === 1 ? 2 : e.button === 2 ? 3 : 1;
    getCoreBridge().sendInput({
      kind: 'button',
      btn,
      down: false,
    });
  };

  const handleContextMenu = (e: MouseEvent) => {
    e.preventDefault(); // 阻止浏览器弹出右键菜单
  };

  const handleWheel = (e: WheelEvent) => {
    e.preventDefault();
    getCoreBridge().sendInput({
      kind: 'scroll',
      dx: e.deltaX,
      dy: e.deltaY,
    });
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    e.preventDefault();
    const sym = keyToSym(e.code, e.key);
    if (sym === 0) return; // 无法映射的键（IME 组合等）跳过，不发送
    let mods = 0;
    if (e.shiftKey) mods |= 1;
    if (e.ctrlKey) mods |= 2;
    if (e.altKey) mods |= 4;
    if (e.metaKey) mods |= 8;
    getCoreBridge().sendInput({
      kind: 'key',
      keycode: sym,
      down: true,
      mods,
    });
  };

  const handleKeyUp = (e: KeyboardEvent) => {
    e.preventDefault();
    const sym = keyToSym(e.code, e.key);
    if (sym === 0) return;
    let mods = 0;
    if (e.shiftKey) mods |= 1;
    if (e.ctrlKey) mods |= 2;
    if (e.altKey) mods |= 4;
    if (e.metaKey) mods |= 8;
    getCoreBridge().sendInput({
      kind: 'key',
      keycode: sym,
      down: false,
      mods,
    });
  };

  return (
    <div
      ref={containerRef}
      tabIndex={0}
      className="pd-video flex min-h-[280px] flex-1 items-center justify-center overflow-hidden rounded-pd-lg border border-pd-border bg-[#07080a] select-none outline-none focus:ring-1 focus:ring-pd-primary/40 cursor-crosshair"
      style={{ aspectRatio: aspect ? `${aspect.w} / ${aspect.h}` : undefined }}
      onMouseMove={handleMouseMove}
      onMouseDown={handleMouseDown}
      onMouseUp={handleMouseUp}
      onContextMenu={handleContextMenu}
      onWheel={handleWheel}
      onKeyDown={handleKeyDown}
      onKeyUp={handleKeyUp}
    >
      {src ? (
        <img
          ref={imgRef}
          src={src}
          alt="远程画面"
          draggable={false}
          className="h-full w-full object-contain pointer-events-none select-none"
        />
      ) : (
        <span className="flex flex-col items-center gap-2 text-[13px] text-pd-muted select-none">
          <IconMonitor size={28} className="opacity-50" />
          等待画面…
        </span>
      )}
    </div>
  );
}
