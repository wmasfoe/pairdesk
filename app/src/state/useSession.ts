/**
 * 会话状态管理：把桥接层（CoreBridge）封装成 React Hook。
 *
 * 职责单一：维护"连接阶段/远端分辨率/错误信息"状态，并转发命令。
 * 页面组件通过 useSession() 拿到状态与操作函数。
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { getCoreBridge } from '../bridge';
import type { CoreEvent } from '../bridge/types';

/** 会话阶段 */
export type SessionPhase =
  | 'idle'
  | 'authentication' // 连接中/认证中
  | 'authenticated'
  | 'connected'
  | 'disconnected'
  | 'error';

export interface SessionState {
  phase: SessionPhase;
  screen: { w: number; h: number } | null;
  error: string | null;
  /** 非致命提示（如打洞端口被占自动顺延） */
  notice: string | null;
  /** 自动择一选中的传输路径（如 QUIC 打洞直连 / 中继兜底） */
  transport: string | null;
}

/** 操作接口（供页面调用） */
export interface SessionControls {
  /** 启动被控端（会话码 + 密码 + 自动就绪） */
  startHostAuto: (relay: string, sid: string, holePort: number, password: string) => void;
  /** 启动控制端（会话码 + 密码 + 自动择一） */
  connectAuto: (relay: string, sid: string, password: string) => void;
  disconnect: () => void;
}

export function useSession(): SessionState & SessionControls {
  const bridgeRef = useRef(getCoreBridge());
  const [state, setState] = useState<SessionState>({
    phase: 'idle',
    screen: null,
    error: null,
    notice: null,
    transport: null,
  });

  useEffect(() => {
    return bridgeRef.current.onEvent((e: CoreEvent) => {
      switch (e.type) {
        case 'authResult':
          setState((s) => ({
            ...s,
            phase: e.ok ? 'authenticated' : 'error',
            error: e.ok ? null : (e.reason ?? '认证失败'),
          }));
          break;
        case 'peerConnected':
          setState((s) => ({ ...s, phase: 'connected' }));
          break;
        case 'peerDisconnected':
          setState((s) => ({ ...s, phase: 'disconnected' }));
          break;
        case 'size':
          setState((s) => ({ ...s, screen: { w: e.w, h: e.h } }));
          break;
        case 'transport':
          setState((s) => ({ ...s, transport: e.path }));
          break;
        case 'error':
          setState((s) => ({ ...s, phase: 'error', error: e.message }));
          break;
        case 'notice':
          setState((s) => ({ ...s, notice: e.message }));
          break;
        default:
          break; // frame/stats 由画面组件单独订阅
      }
    });
  }, []);

  const startHostAuto = useCallback(
    (relay: string, sid: string, holePort: number, password: string) => {
      setState({ phase: 'authentication', screen: null, error: null, notice: null, transport: null });
      void bridgeRef.current.startHostAuto({ relay, sid, holePort, password });
    },
    [],
  );

  const connectAuto = useCallback((relay: string, sid: string, password: string) => {
    setState({ phase: 'authentication', screen: null, error: null, notice: null, transport: null });
    void bridgeRef.current.connectAuto({ relay, sid, password });
  }, []);

  const disconnect = useCallback(() => {
    bridgeRef.current.stop();
    setState((s) => ({ ...s, phase: 'disconnected' }));
  }, []);

  return { ...state, startHostAuto, connectAuto, disconnect };
}
