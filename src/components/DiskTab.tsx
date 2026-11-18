import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { formatBytes } from '../lib/format';
import { errorMessage } from '../lib/tauri-stream';
import type { CleanupExecutionReport, CleanupRuleFinding, CleanupRuleId, CleanupScanReport, SpaceAnalysisReport, VolumeUsage } from '../types';
import { SmartCleanerView } from './disk/SmartCleanerView';
import { SpaceAnalyzerView } from './disk/SpaceAnalyzerView';

const DEFAULT_TARGET = 'downloads';
const isRuleActive = (rule: CleanupRuleFinding) => rule.exists && (rule.bytes > 0 || rule.fileCount > 0);

function syncRuleSelection(prev: Set<CleanupRuleId>, rules: CleanupRuleFinding[], isInitial: boolean): Set<CleanupRuleId> {
  const activeIds = new Set(rules.filter(isRuleActive).map(r => r.id));
  if (isInitial) return new Set(rules.filter(r => isRuleActive(r) && r.category === 'Hệ thống').map(r => r.id));
  return new Set([...prev].filter(id => activeIds.has(id)));
}

export function DiskTab({ volumes }: { volumes: VolumeUsage[] }) {
  const [target, setTarget] = useState(DEFAULT_TARGET);
  const [subtab, setSubtab] = useState<'files' | 'artifacts'>('files');
  const [spaceReport, setSpaceReport] = useState<SpaceAnalysisReport | null>(null);
  const [spaceLoading, setSpaceLoading] = useState(false);
  const [spaceError, setSpaceError] = useState<string | null>(null);
  const [spaceNotice, setSpaceNotice] = useState<string | null>(null);
  const [cleanReport, setCleanReport] = useState<CleanupScanReport | null>(null);
  const [selected, setSelected] = useState<Set<CleanupRuleId>>(() => new Set());
  const [cleanLoading, setCleanLoading] = useState(false);
  const [cleaning, setCleaning] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [cleanError, setCleanError] = useState<string | null>(null);
  const [cleanNotice, setCleanNotice] = useState<string | null>(null);

  const spacePendingRef = useRef(false);
  const cleanPendingRef = useRef(false);
  const disposedRef = useRef(false);
  const initializedCleanRef = useRef(false);

  const scanSpace = useCallback(async (nextTarget: string) => {
    if (spacePendingRef.current) return;
    spacePendingRef.current = true;
    setSpaceLoading(true);
    setSpaceError(null);
    setSpaceNotice(null);
    try {
      const report = await invoke<SpaceAnalysisReport>('analyze_disk_space', { target: nextTarget });
      if (!disposedRef.current) setSpaceReport(report);
    } catch (failure) {
      if (!disposedRef.current) setSpaceError(errorMessage(failure));
    } finally {
      spacePendingRef.current = false;
      if (!disposedRef.current) setSpaceLoading(false);
    }
  }, []);

  const scanCleaner = useCallback(async () => {
    if (cleanPendingRef.current) return;
    cleanPendingRef.current = true;
    setCleanLoading(true);
    setCleanError(null);
    try {
      const report = await invoke<CleanupScanReport>('scan_cleanup_rules');
      if (!disposedRef.current) {
        const isInitial = !initializedCleanRef.current;
        initializedCleanRef.current = true;
        setCleanReport(report);
        setSelected(prev => syncRuleSelection(prev, report.rules, isInitial));
        setConfirming(false);
      }
    } catch (failure) {
      if (!disposedRef.current) setCleanError(errorMessage(failure));
    } finally {
      cleanPendingRef.current = false;
      if (!disposedRef.current) setCleanLoading(false);
    }
  }, []);

  useEffect(() => {
    disposedRef.current = false;
    void scanSpace(DEFAULT_TARGET);
    void scanCleaner();
    return () => { disposedRef.current = true; };
  }, [scanSpace, scanCleaner]);

  const handleSelectTarget = useCallback((nextTarget: string) => {
    setTarget(nextTarget);
    void scanSpace(nextTarget);
  }, [scanSpace]);

  const handleToggleRule = useCallback((id: CleanupRuleId) => {
    setConfirming(false);
    setSelected(prev => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  }, []);

  const handleConfirmClean = useCallback(async () => {
    if (cleanPendingRef.current || selected.size === 0) return;
    cleanPendingRef.current = true;
    setCleaning(true);
    setCleanError(null);
    setCleanNotice(null);
    try {
      const result = await invoke<CleanupExecutionReport>('execute_cleanup_rules', { rules: [...selected] });
      if (!disposedRef.current) {
        setCleanReport(result.refreshed);
        setSelected(prev => syncRuleSelection(prev, result.refreshed.rules, false));
        setConfirming(false);
        const skipped = result.skippedLockedFiles > 0 ? ` · bỏ qua ${result.skippedLockedFiles} tệp đang mở` : '';
        setCleanNotice(`Đã giải phóng ${formatBytes(result.freedBytes)} (${result.deletedFiles} tệp)${skipped}.`);
      }
    } catch (failure) {
      if (!disposedRef.current) setCleanError(errorMessage(failure));
    } finally {
      cleanPendingRef.current = false;
      if (!disposedRef.current) setCleaning(false);
    }
  }, [selected]);

  const handleRevealPath = useCallback(async (path: string) => {
    setSpaceError(null);
    try {
      const opened = await invoke<string>('reveal_disk_path', { path });
      if (!disposedRef.current) setSpaceNotice(`Đã mở: ${opened}`);
    } catch (failure) {
      if (!disposedRef.current) setSpaceError(errorMessage(failure));
    }
  }, []);

  return (
    <div className="disk-layout">
      <SpaceAnalyzerView volumes={volumes} target={target} report={spaceReport} subtab={subtab} loading={spaceLoading} error={spaceError} notice={spaceNotice} onSelectTarget={handleSelectTarget} onRescan={() => void scanSpace(target)} onSelectSubtab={setSubtab} onRevealPath={path => void handleRevealPath(path)} />
      <SmartCleanerView report={cleanReport} selected={selected} loading={cleanLoading} cleaning={cleaning} confirming={confirming} error={cleanError} notice={cleanNotice} onRescan={() => void scanCleaner()} onToggleRule={handleToggleRule} onRequestClean={() => setConfirming(true)} onConfirmClean={() => void handleConfirmClean()} onCancelClean={() => setConfirming(false)} onRevealPath={path => void handleRevealPath(path)} />
    </div>
  );
}
