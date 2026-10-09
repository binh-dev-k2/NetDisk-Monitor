import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTauriStream } from './useTauriStream';
import { errorMessage } from '../lib/tauri-stream';
import { sameProcessMetrics } from '../lib/process-model';
import type { History, ProcessSnapshot, Snapshot, SystemSnapshot } from '../types';

export const emptySnapshot: SystemSnapshot = {
  sequence: 0, diskAvailable: false, networkDownBps: 0, networkUpBps: 0, diskReadBps: 0, diskWriteBps: 0,
  sessionNetworkDown: 0, sessionNetworkUp: 0, sessionDiskRead: 0, sessionDiskWrite: 0,
  sessionDurationSecs: 0, interfaces: [], totalMemoryBytes: 0, usedMemoryBytes: 0,
};
export const snapshotSequence = (snapshot: SystemSnapshot) => snapshot.sequence;
export function systemSnapshot(value: SystemSnapshot & Partial<ProcessSnapshot>): SystemSnapshot {
  const { processes: _processes, processSequence: _processSequence, processNetworkError: _processNetworkError, ...system } = value;
  return system;
}
const fetchProcesses = async (): Promise<ProcessSnapshot> => {
  const { processSequence, processes, processNetworkError } = await invoke<Snapshot>('get_snapshot');
  return { processSequence, processes, processNetworkError };
};
const processSequence = (snapshot: ProcessSnapshot) => snapshot.processSequence;
const sameProcesses = (left: ProcessSnapshot, right: ProcessSnapshot) => left.processNetworkError === right.processNetworkError && sameProcessMetrics(left.processes, right.processes);
const emptyProcesses: ProcessSnapshot = { processSequence: 0, processes: [] };
const emptyHistory: History = { down: [], up: [], read: [], write: [] };

export function useMonitorSnapshot() {
  const [snapshot, , status] = useTauriStream<SystemSnapshot>('get_snapshot', 'metrics://snapshot', {
    defaultValue: emptySnapshot, normalize: systemSnapshot, sequence: snapshotSequence,
    transition: true, staleAfter: 3500, pollInterval: 1000, pauseWhenHidden: true,
  });
  const [processSnapshot, , processStatus] = useTauriStream<ProcessSnapshot>('get_snapshot', 'processes://snapshot', {
    defaultValue: emptyProcesses, fetch: fetchProcesses, equal: sameProcesses, sequence: processSequence,
    transition: true, staleAfter: 12000, pollInterval: 5000, pauseWhenHidden: true,
  });
  const [history, setHistory] = useState<History>(emptyHistory);
  const [resetError, setResetError] = useState<string | null>(null);
  const [resetPending, setResetPending] = useState(false);
  const resetInFlight = useRef(false);
  useEffect(() => {
    if (snapshot.sequence === 0) return;
    setHistory(prev => ({
      down: [...prev.down.slice(-29), snapshot.networkDownBps],
      up: [...prev.up.slice(-29), snapshot.networkUpBps],
      read: [...prev.read.slice(-29), snapshot.diskReadBps],
      write: [...prev.write.slice(-29), snapshot.diskWriteBps],
    }));
  }, [snapshot]);
  const resetSession = async () => {
    if (resetInFlight.current) return;
    resetInFlight.current = true;
    setResetPending(true);
    setResetError(null);
    try { await invoke('reset_session_totals'); }
    catch (error) { setResetError(errorMessage(error)); }
    finally { resetInFlight.current = false; setResetPending(false); }
  };
  return { snapshot, processes: processSnapshot.processes, processNetworkError: processSnapshot.processNetworkError, history, status, processStatus, resetSession, resetError, resetPending };
}
