import { startTransition, useEffect, useRef, useState, type Dispatch, type SetStateAction } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { subscribeToStream } from '../lib/tauri-stream';

type UseTauriStreamOptions<T> = {
  defaultValue: T;
  normalize?: (value: T) => T;
  fetch?: () => Promise<T>;
  sequence?: (value: T) => number;
  equal?: (left: T, right: T) => boolean;
  transition?: boolean;
  staleAfter?: number;
  pollInterval?: number;
  enabled?: boolean;
  pauseWhenHidden?: boolean;
};
export type StreamStatus = { error: string | null; stale: boolean; loading: boolean };

export function useTauriStream<T>(command: string, eventName: string, options: UseTauriStreamOptions<T>): [T, Dispatch<SetStateAction<T>>, StreamStatus] {
  const { defaultValue, normalize, fetch, sequence, equal, transition = false, staleAfter, pollInterval, enabled = true, pauseWhenHidden = false } = options;
  const [value, setValue] = useState(defaultValue);
  const [status, setStatus] = useState<StreamStatus>({ error: null, stale: false, loading: true });
  const receivedAt = useRef(performance.now());
  const [documentVisible, setDocumentVisible] = useState(() => typeof document === 'undefined' || document.visibilityState !== 'hidden');
  useEffect(() => {
    if (!pauseWhenHidden) return;
    const update = () => setDocumentVisible(document.visibilityState !== 'hidden');
    document.addEventListener('visibilitychange', update);
    return () => document.removeEventListener('visibilitychange', update);
  }, [pauseWhenHidden]);
  useEffect(() => {
    if (!enabled || (pauseWhenHidden && !documentVisible)) return;
    receivedAt.current = performance.now();
    const prepare = (next: T) => normalize ? normalize(next) : next;
    const stop = subscribeToStream<T>({
      fetch: async () => prepare(await (fetch ? fetch() : invoke<T>(command))),
      listen: handler => listen<T>(eventName, event => handler(prepare(event.payload))),
      sequence, equal, pollInterval, staleAfter,
      onActivity: () => {
        receivedAt.current = performance.now();
        setStatus(prev => prev.error || prev.stale || prev.loading ? { error: null, stale: false, loading: false } : prev);
      },
      onError: error => setStatus(prev => ({ ...prev, error, loading: false })),
      onValue: next => {
        receivedAt.current = performance.now();
        const update = () => { setValue(next); setStatus({ error: null, stale: false, loading: false }); };
        if (transition) startTransition(update); else update();
      },
    });
    const timer = staleAfter ? setInterval(() => {
      const stale = performance.now() - receivedAt.current >= staleAfter;
      setStatus(prev => prev.stale === stale ? prev : { ...prev, stale });
    }, 1000) : undefined;
    return () => { stop(); if (timer) clearInterval(timer); };
  }, [command, eventName, normalize, fetch, sequence, equal, transition, staleAfter, pollInterval, enabled, pauseWhenHidden, documentVisible]);
  return [value, setValue, status];
}
