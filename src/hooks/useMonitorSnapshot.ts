import { useEffect, useState, startTransition } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { History, Snapshot } from '../types';

const emptySnapshot: Snapshot = {
  networkDownBps: 0,
  networkUpBps: 0,
  diskReadBps: 0,
  diskWriteBps: 0,
  sessionNetworkDown: 0,
  sessionNetworkUp: 0,
  sessionDiskRead: 0,
  sessionDiskWrite: 0,
  sessionDurationSecs: 0,
  processes: [],
  interfaces: [],
  totalMemoryBytes: 0,
};

const emptyHistory: History = { down: [], up: [], read: [], write: [] };

export function useMonitorSnapshot() {
  const [snapshot, setSnapshot] = useState<Snapshot>(emptySnapshot);
  const [history, setHistory] = useState<History>(emptyHistory);
  const [sessionNetwork, setSessionNetwork] = useState<Record<string, { down: number; up: number }>>({});

  useEffect(() => {
    const updateSnapshot = (current: Snapshot) => {
      startTransition(() => {
        setSnapshot(current);

        setSessionNetwork(prev => {
          const next = { ...prev };
          for (const p of current.processes) {
            const pidKey = `${p.name}-${p.pid}`;
            const existing = next[pidKey] || { down: 0, up: 0 };
            next[pidKey] = {
              down: existing.down + p.networkDownBps,
              up: existing.up + p.networkUpBps,
            };
          }
          return next;
        });

        setHistory(prev => ({
          down: [...prev.down.slice(-29), current.networkDownBps],
          up: [...prev.up.slice(-29), current.networkUpBps],
          read: [...prev.read.slice(-29), current.diskReadBps],
          write: [...prev.write.slice(-29), current.diskWriteBps],
        }));
      });
    };

    // Initial fetch
    void invoke<Snapshot>('get_snapshot').then(updateSnapshot).catch(console.error);

    // Listen to real-time events from Tauri backend
    const unlistenPromise = listen<Snapshot>('metrics://snapshot', event => {
      updateSnapshot(event.payload);
    });

    // 1s Interval Polling Fallback to guarantee continuous updates
    const timer = setInterval(() => {
      void invoke<Snapshot>('get_snapshot').then(updateSnapshot).catch(console.error);
    }, 1000);

    return () => {
      clearInterval(timer);
      void unlistenPromise.then(f => f());
    };
  }, []);

  const resetSession = () => {
    void invoke('reset_session_totals');
    setSessionNetwork({});
  };

  return { snapshot, history, sessionNetwork, resetSession };
}
