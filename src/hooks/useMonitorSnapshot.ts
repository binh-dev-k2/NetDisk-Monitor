import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTauriStream } from './useTauriStream';
import { errorMessage } from '../lib/tauri-stream';
import { sameProcessMetrics } from '../lib/process-model';
import type { History, ProcessSnapshot, SystemSnapshot } from '../types';

export const emptySnapshot: SystemSnapshot = {
  sequence: 0, activeProcessCount: null, diskAvailable: false, networkDownBps: 0, networkUpBps: 0, diskReadBps: 0, diskWriteBps: 0,
  sessionNetworkDown: 0, sessionNetworkUp: 0, sessionDiskRead: 0, sessionDiskWrite: 0,
  sessionDurationSecs: 0, interfaces: [], totalMemoryBytes: 0, usedMemoryBytes: 0,
};
export const snapshotSequence = (snapshot: SystemSnapshot) => snapshot.sequence;
export function systemSnapshot(value: SystemSnapshot & Partial<ProcessSnapshot>): SystemSnapshot {
  const { processes: _processes, processSequence: _processSequence, processNetworkError: _processNetworkError, ...system } = value;
  return system;
}
export function appendHistorySample(prev: History, snapshot: SystemSnapshot): History {
  return {
    down: [...prev.down.slice(-29), snapshot.networkDownBps],
    up: [...prev.up.slice(-29), snapshot.networkUpBps],
    read: [...prev.read.slice(-29), snapshot.diskReadBps],
    write: [...prev.write.slice(-29), snapshot.diskWriteBps],
  };
}
const processSequence = (snapshot: ProcessSnapshot) => snapshot.processSequence;
const sameProcesses = (left: ProcessSnapshot, right: ProcessSnapshot) => left.processNetworkError === right.processNetworkError && sameProcessMetrics(left.processes, right.processes);
const emptyProcesses: ProcessSnapshot = { processSequence: 0, processes: [] };
const emptyHistory: History = { down: [], up: [], read: [], write: [] };

export function useMonitorSnapshot(processesEnabled = true) {
  const [history, setHistory] = useState<History>(emptyHistory);
  const lastHistorySeqRef = useRef(0);
  const [systemRetry, setSystemRetry] = useState(0);
  const [processRetry, setProcessRetry] = useState(0);
  const [snapshot, , status] = useTauriStream<SystemSnapshot>('get_system_snapshot', 'metrics://snapshot', {
    defaultValue: emptySnapshot, normalize: systemSnapshot, sequence: snapshotSequence,
    onCommit: next => {
      if (next.sequence > lastHistorySeqRef.current) {
        lastHistorySeqRef.current = next.sequence;
        setHistory(prev => appendHistorySample(prev, next));
      }
    },
    retryKey: systemRetry,
    transition: true, staleAfter: 3500, pollInterval: 1000, pauseWhenHidden: true,
  });
  const [processSnapshot, , processStatus] = useTauriStream<ProcessSnapshot>('get_process_snapshot', 'processes://snapshot', {
    defaultValue: emptyProcesses, equal: sameProcesses, sequence: processSequence, enabled: processesEnabled,
    retryKey: processRetry,
    transition: true, staleAfter: 12000, pollInterval: 5000, pauseWhenHidden: true,
  });
  const [resetError, setResetError] = useState<string | null>(null);
  const [resetPending, setResetPending] = useState(false);
  const resetInFlight = useRef(false);
  useEffect(() => {
    void invoke('set_process_view_active', { active: processesEnabled }).catch(() => {});
    return () => {
      if (processesEnabled) {
        void invoke('set_process_view_active', { active: false }).catch(() => {});
      }
    };
  }, [processesEnabled]);
  const retrySystem = useCallback(() => setSystemRetry(value => value + 1), []);
  const retryProcesses = useCallback(() => setProcessRetry(value => value + 1), []);
  const resetSession = async () => {
    if (resetInFlight.current) return;
    resetInFlight.current = true;
    setResetPending(true);
    setResetError(null);
    try { await invoke('reset_session_totals'); }
    catch (error) { setResetError(errorMessage(error)); }
    finally { resetInFlight.current = false; setResetPending(false); }
  };
  return { snapshot, processes: processSnapshot.processes, processNetworkError: processSnapshot.processNetworkError, history, status, processStatus, resetSession, resetError, resetPending, retrySystem, retryProcesses };
}
