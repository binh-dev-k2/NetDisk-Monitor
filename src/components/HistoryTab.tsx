import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { ChevronLeft, ChevronRight, Download, RefreshCw } from 'lucide-react';
import { HistoryKpiStrip } from './history/HistoryKpiStrip';
import { buildHistoryCsv, historyRate, type HistoryRecord } from '../lib/history';
import { formatBytes, formatDuration, formatRate } from '../lib/format';
import { errorMessage } from '../lib/tauri-stream';

export function HistoryTab({ active, diskAvailable }: { active: boolean; diskAvailable: boolean }) {
  const [resolution, setResolution] = useState<'minute' | 'day'>('day');
  const [records, setRecords] = useState<HistoryRecord[]>([]);
  const [page, setPage] = useState(0);
  const [loading, setLoading] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const exportPending = useRef(false);
  const lastLoadedAt = useRef(0);
  const [reloadKey, setReloadKey] = useState(0);

  const reload = useCallback(() => setReloadKey(v => v + 1), []);

  useEffect(() => {
    if (!active) return;
    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    setLoading(true);
    setPage(0);

    const schedule = () => {
      if (disposed || document.visibilityState === 'hidden') return;
      timer = setTimeout(() => { void load(); }, 60_000);
    };

    const load = async () => {
      if (disposed) return;
      try {
        const limit = resolution === 'minute' ? 120 : 366;
        const data = await invoke<HistoryRecord[]>('get_history', { resolution, limit });
        if (!disposed) { setRecords(data); setError(null); lastLoadedAt.current = performance.now(); }
      } catch (failure) {
        if (!disposed) setError(errorMessage(failure));
      } finally {
        if (!disposed) { setLoading(false); schedule(); }
      }
    };

    const onVisibility = () => {
      clearTimeout(timer);
      if (document.visibilityState !== 'hidden') {
        if (performance.now() - lastLoadedAt.current >= 60_000) void load();
        else schedule();
      }
    };

    let unlisten: (() => void) | undefined;
    void listen('history://updated', () => {
      if (disposed) return;
      lastLoadedAt.current = 0;
      void load();
    }).then(fn => {
      if (disposed) fn();
      else unlisten = fn;
    });

    document.addEventListener('visibilitychange', onVisibility);
    void load();
    return () => {
      disposed = true;
      clearTimeout(timer);
      unlisten?.();
      document.removeEventListener('visibilitychange', onVisibility);
    };
  }, [active, resolution, reloadKey]);

  if (!active) return null;

  const exportCsv = async () => {
    if (exportPending.current) return;
    exportPending.current = true;
    setExporting(true);
    setNotice(null);
    setError(null);
    try {
      const limit = resolution === 'minute' ? 10_080 : 366;
      const data = await invoke<HistoryRecord[]>('get_history', { resolution, limit });
      const filename = `NetDisk_History_${resolution}_${new Date().toISOString().slice(0, 10)}.csv`;
      const savedPath = await invoke<string>('export_csv_file', { filename, content: buildHistoryCsv(data, diskAvailable) });
      setNotice(`Đã lưu CSV: ${savedPath}`);
    } catch (failure) { setError(errorMessage(failure)); }
    finally { exportPending.current = false; setExporting(false); }
  };

  const pages = Math.max(1, Math.ceil(records.length / 24));
  const currentPage = Math.min(page, pages - 1);

  return (
    <section className="history-section view-panel">
      <div className="history-toolbar">
        <div>
          <h3>Lịch sử tổng máy</h3>
          <p className="panel-subtitle">Kho nhị phân cố định 1,347,584 byte · Giữ 7 ngày theo phút &amp; 366 ngày UTC · Hiển thị giờ địa phương</p>
        </div>
        <div className="history-controls">
          <select aria-label="Độ chi tiết lịch sử" value={resolution} onChange={event => { setRecords([]); setResolution(event.target.value as 'minute' | 'day'); }}>
            <option value="day">Ngày · 366 ngày gần nhất</option>
            <option value="minute">Phút · 120 phút gần nhất</option>
          </select>
          <button type="button" title="Làm mới lịch sử" aria-label="Làm mới lịch sử" disabled={loading} onClick={reload}><RefreshCw size={15} /></button>
          <button type="button" title="Xuất toàn bộ lịch sử đã giữ" aria-label="Xuất lịch sử CSV" disabled={exporting || loading || !records.length} onClick={() => void exportCsv()}><Download size={15} /></button>
        </div>
      </div>
      {error && <div className="diagnostics diagnostics-row" role="alert"><span>{error}</span><button type="button" className="inline-action-btn" onClick={reload}>Thử lại</button></div>}
      {notice && <div className="notice" role="status">{notice}</div>}
      <HistoryKpiStrip records={records} diskAvailable={diskAvailable} />
      <div className="history-scroll" role="region" tabIndex={0} aria-label="Bảng lịch sử">
        <table>
          <thead><tr><th scope="col">Thời điểm</th><th scope="col">Đã đo</th><th scope="col">Tải xuống</th><th scope="col">Tải lên</th><th scope="col">I/O đọc</th><th scope="col">I/O ghi</th></tr></thead>
          <tbody>
            {records.slice(currentPage * 24, (currentPage + 1) * 24).map(record => (
              <tr key={record.timestampSecs}>
                <td>{new Date(record.timestampSecs * 1000).toLocaleString('vi-VN', resolution === 'day' ? { dateStyle: 'short' } : { dateStyle: 'short', timeStyle: 'short' })}</td>
                <td>{formatDuration(record.durationMillis / 1000)}</td>
                {record.bytes.map((bytes, index) => {
                  const rate = historyRate(bytes, record.durationMillis);
                  const available = index < 2 || diskAvailable;
                  return <td key={index} title={available && rate !== null ? formatRate(rate) : undefined}>{available ? formatBytes(bytes) : 'N/A'}</td>;
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {!records.length && <div className="empty">
        <p>{loading ? 'Đang đọc lịch sử...' : 'Chưa có dữ liệu lịch sử.'}</p>
        {!loading && <button type="button" className="inline-action-btn" onClick={reload}>Làm mới</button>}
      </div>}
      <div className="history-pagination">
        <span>{records.length} bản ghi</span>
        <button type="button" title="Trang lịch sử trước" aria-label="Trang lịch sử trước" disabled={currentPage === 0} onClick={() => setPage(currentPage - 1)}><ChevronLeft size={15} /></button>
        <span>{currentPage + 1} / {pages}</span>
        <button type="button" title="Trang lịch sử sau" aria-label="Trang lịch sử sau" disabled={currentPage >= pages - 1} onClick={() => setPage(currentPage + 1)}><ChevronRight size={15} /></button>
      </div>
    </section>
  );
}
