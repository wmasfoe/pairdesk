/**
 * 会话状态管理：把桥接层（CoreBridge）封装成 React Hook。
 *
 * 职责单一：维护"连接阶段/远端分辨率/错误信息"状态，并转发命令。
 * 不包含任何渲染；页面组件通过 useSession() 拿到状态与操作函数。
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { getCoreBridge } from '../bridge';
import type { CoreEvent } from '../bridge/types';

/** 会话阶段 */
export type SessionPhase =
  | 'idle' // 尚未发起
  | 'authentication' // 连接中/认证中
  | 'authenticated' // 认证成功
  | 'connected' // 画面流进行中
  | 'disconnected' // 已断开
  | 'error'; // 出错

export interface SessionState {
  phase: SessionPhase;
  screen: { w: number; h: number } | null;
  error: string | null;
}

/** 操作接口（供页面调用） */
export interface SessionControls {
  startHost: (port: number, password: string) => void;
  connect: (addr: string, password: string) => void;
  disconnect: () => void;
}

export function useSession(): SessionState & SessionControls {
  const bridgeRef = useRef(getCoreBridge());
  const [state, setState] = useState<SessionState>({
    phase: 'idle',
    screen: null,
    error: null,
  });

  // 事件驱动状态机：桥接层每推一个事件，按事件更新阶段
  useEffect(() => {
    return bridgeRef.current.onEvent((e: CoreEvent) => {
      switch (e.type) {
        case 'auth-result':
          setState((s) => ({
            ...s,
            phase: e.ok ? 'authenticated' : 'error',
            error: e.ok ? null : (e.reason ?? '认证失败'),
          }));
          break;
        case 'peer-connected':
          setState((s) => ({ ...s, phase: 'connected' }));
          break;
        case 'peer-disconnected':
          setState((s) => ({ ...s, phase: 'disconnected' }));
          break;
        case 'size':
          setState((s) => ({ ...s, screen: { w: e.w, h: e.h } }));
          break;
        case 'error':
          setState((s) => ({ ...s, phase: 'error', error: e.message }));
          break;
        default:
          break; // screen-frame/stats 由画面组件单独订阅
      }
    });
  }, []);

  const startHost = useCallback((port: number, password: string) => {
    setState({ phase: 'authentication', screen: null, error: null });
    void bridgeRef.current.startHost(port, password);
  }, []);

  const connect = useCallback((addr: string, password: string) => {
    setState({ phase: 'authentication', screen: null, error: null });
    void bridgeRef.current.connect(addr, password);
  }, []);

  const disconnect = useCallback(() => {
    bridgeRef.current.stop();
    setState((s) => ({ ...s, phase: 'disconnected' }));
  }, []);

  return { ...state, startHost, connect, disconnect };
}
