import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { RefreshCw } from 'lucide-react';
import { errorMessage } from '../lib/tauri-stream';
import type { RuntimeHealth, SystemSnapshot, WorkerHealthItem } from '../types';

const WORKER_LABELS: Record<WorkerHealthItem['kind'], string> = {
  system: 'Thu thập hệ thống (1s)',
  processes: 'Thu thập tiến trình (5s)',
  storage: 'Lưu trữ & cấu hình (1s)',
};

const WORKER_STATUS_LABELS: Record<WorkerHealthItem['status'], string> = {
  notStarted: 'Chưa chạy',
  running: 'Đang chạy',
  warning: 'Cảnh báo',
  failed: 'Lỗi',
  stopped: 'Đã dừng',
};

export function DiagnosticsPanel({ active, snapshot }: { active: boolean; snapshot: SystemSnapshot }) {
  const [health, setHealth] = useState<RuntimeHealth | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);

  const reload = useCallback(() => setRefreshKey(v => v + 1), []);

  useEffect(() => {
    if (!active) return;
    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    setLoading(true);

    const schedule = () => {
      if (disposed || document.visibilityState === 'hidden') return;
      timer = setTimeout(() => { void load(); }, 5_000);
    };

    const load = async () => {
      if (disposed) return;
      try {
        const result = await invoke<RuntimeHealth>('get_runtime_health');
        if (!disposed) { setHealth(result); setError(null); }
      } catch (failure) {
        if (!disposed) setError(errorMessage(failure));
      } finally {
        if (!disposed) { setLoading(false); schedule(); }
      }
    };

    const onVisibility = () => {
      clearTimeout(timer);
      if (document.visibilityState !== 'hidden') void load();
    };

    document.addEventListener('visibilitychange', onVisibility);
    void load();
    return () => {
      disposed = true;
      clearTimeout(timer);
      document.removeEventListener('visibilitychange', onVisibility);
    };
  }, [active, refreshKey]);

  return (
    <section className="diagnostics-panel" aria-label="Chẩn đoán hệ thống">
      <div className="diagnostics-header">
        <div>
          <h3>Chẩn đoán hoạt động 24/7</h3>
          <p className="panel-subtitle">Trạng thái luồng nền, nhật ký giới hạn và nguồn đo</p>
        </div>
        <button type="button" className="icon-button-text" onClick={reload} disabled={loading} aria-label="Làm mới chẩn đoán">
          <RefreshCw size={13} /> Làm mới
        </button>
      </div>

      {error && (
        <div className="diagnostics diagnostics-row" role="alert">
          <span>{error}</span>
          <button type="button" className="inline-action-btn" onClick={reload}>Thử lại</button>
        </div>
      )}
      {loading && !health && !error && <div className="empty">Đang đọc trạng thái runtime...</div>}
      {!loading && !health && !error && <div className="empty">Không có dữ liệu chẩn đoán.</div>}

      {health && (
        <>
          <div className="diagnostics-meta">
            <span>PID: <b>{health.processId}</b></span>
            <span>Yêu cầu thoát bị chặn: <b>{health.ignoredExitRequests}</b></span>
            <span>
              Log chẩn đoán:{' '}
              <b>
                {health.logging
                  ? `${health.logging.active ? 'Hoạt động' : 'Tạm dừng'} · Bỏ ${health.logging.droppedEvents}`
                  : 'Chưa khởi tạo'}
              </b>
            </span>
          </div>
          {health.logging?.lastError && <div className="diagnostics" role="alert">{health.logging.lastError}</div>}
          {snapshot.storageError && <div className="diagnostics" role="alert">{snapshot.storageError}</div>}
          {snapshot.processNetworkStatusError && <div className="notice" role="status">{snapshot.processNetworkStatusError}</div>}

          <div className="worker-grid">
            {health.workers.map(worker => (
              <div className="worker-card" key={worker.kind}>
                <div className="worker-card-top">
                  <strong>{WORKER_LABELS[worker.kind]}</strong>
                  <span className={`status-badge ${worker.status === 'running' ? 'ok' : worker.status === 'failed' ? 'err' : 'warn'}`}>
                    {WORKER_STATUS_LABELS[worker.status]}
                  </span>
                </div>
                <div className="worker-card-meta">
                  <span>Nhịp: {worker.ticks}</span>
                  <span>Trễ: {worker.heartbeatAgeMillis === null ? 'N/A' : `${worker.heartbeatAgeMillis} ms`}</span>
                </div>
                {worker.lastError && <small className="worker-error">{worker.lastError}</small>}
              </div>
            ))}
          </div>
        </>
      )}
    </section>
  );
}
