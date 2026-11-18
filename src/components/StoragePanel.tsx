import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { errorMessage } from '../lib/tauri-stream';
import type { ClearTarget, StorageOverview } from '../types';
import { StorageView } from './storage/StorageView';

const CLEAR_NOTICES: Record<ClearTarget, string> = {
  diagnostics: 'Đã dọn sạch nhật ký chẩn đoán.',
  caches: 'Đã dọn cache icon RAM, tiến trình đã thoát và cấu hình v1 cũ.',
  history: 'Đã xóa toàn bộ lịch sử lưu lượng và giữ nguyên file vòng đệm.',
  all_clearable: 'Đã dọn toàn bộ nhật ký, cache RAM và lịch sử cũ.',
};

export function StoragePanel({ active }: { active: boolean }) {
  const [overview, setOverview] = useState<StorageOverview | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [pendingAction, setPendingAction] = useState<string | null>(null);
  const [confirmTarget, setConfirmTarget] = useState<'history' | 'all_clearable' | null>(null);
  const pendingRef = useRef(false);
  const disposedRef = useRef(false);

  const load = useCallback(async () => {
    if (pendingRef.current) return;
    pendingRef.current = true;
    setLoading(true);
    try {
      const next = await invoke<StorageOverview>('scan_storage_info');
      if (!disposedRef.current) {
        setOverview(next);
        setError(null);
      }
    } catch (failure) {
      if (!disposedRef.current) setError(errorMessage(failure));
    } finally {
      pendingRef.current = false;
      if (!disposedRef.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    disposedRef.current = false;
    if (active) void load();
    else setConfirmTarget(null);
    return () => { disposedRef.current = true; };
  }, [active, load]);

  const executeClear = useCallback(async (target: ClearTarget) => {
    if (pendingRef.current) return;
    pendingRef.current = true;
    setPendingAction(target);
    setConfirmTarget(null);
    setNotice(null);
    setError(null);
    try {
      const next = await invoke<StorageOverview>('clear_storage_data', { target });
      if (!disposedRef.current) {
        setOverview(next);
        setNotice(CLEAR_NOTICES[target]);
      }
      if (target === 'caches' || target === 'all_clearable') {
        window.dispatchEvent(new CustomEvent('netdisk:caches-cleared'));
      }
    } catch (failure) {
      if (!disposedRef.current) setError(errorMessage(failure));
    } finally {
      pendingRef.current = false;
      if (!disposedRef.current) setPendingAction(null);
    }
  }, []);

  const handleRequestClear = useCallback((target: ClearTarget) => {
    if (target === 'history' || target === 'all_clearable') {
      setConfirmTarget(target);
      return;
    }
    void executeClear(target);
  }, [executeClear]);

  const handleOpenFolder = useCallback(async (target: 'app_data' | 'webview') => {
    if (pendingRef.current) return;
    pendingRef.current = true;
    setPendingAction(`open:${target}`);
    setError(null);
    try {
      const opened = await invoke<string>('open_storage_folder', { target });
      if (!disposedRef.current) setNotice(`Đã mở thư mục: ${opened}`);
    } catch (failure) {
      if (!disposedRef.current) setError(errorMessage(failure));
    } finally {
      pendingRef.current = false;
      if (!disposedRef.current) setPendingAction(null);
    }
  }, []);

  return (
    <StorageView
      overview={overview}
      loading={loading}
      error={error}
      notice={notice}
      pendingAction={pendingAction}
      confirmTarget={confirmTarget}
      onRefresh={() => { setNotice(null); void load(); }}
      onOpenFolder={target => void handleOpenFolder(target)}
      onRequestClear={handleRequestClear}
      onConfirmClear={target => void executeClear(target)}
      onCancelConfirm={() => setConfirmTarget(null)}
    />
  );
}
