import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ChevronLeft, ChevronRight, Download, RefreshCw } from 'lucide-react';
import { buildHistoryCsv, historyRate, type HistoryRecord } from '../../lib/history';
import { formatBytes, formatDuration, formatRate } from '../../lib/format';
import { errorMessage } from '../../lib/tauri-stream';

export function HistoryPanel({ diskAvailable }: { diskAvailable: boolean }) {
  const [resolution, setResolution] = useState<'minute' | 'day'>('day');
  const [records, setRecords] = useState<HistoryRecord[]>([]);
  const [page, setPage] = useState(0);
  const [refresh, setRefresh] = useState(0);
  const [loading, setLoading] = useState(true);
  const [exporting, setExporting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const exportPending = useRef(false);
  useEffect(() => {
    let disposed = false;
    let timer: ReturnType<typeof setTimeout>;
    setLoading(true);
    setRecords([]);
    setPage(0);
    const load = async () => {
      try {
        const data = await invoke<HistoryRecord[]>('get_history', { resolution, limit: resolution === 'minute' ? 120 : 366 });
        if (!disposed) { setRecords(data); setError(null); }
      } catch (failure) { if (!disposed) setError(errorMessage(failure)); }
      finally {
        if (!disposed) {
          setLoading(false);
          timer = setTimeout(() => { if (!document.hidden) void load(); else timer = setTimeout(load, 60000); }, 60000);
        }
      }
    };
    void load();
    return () => { disposed = true; clearTimeout(timer); };
  }, [resolution, refresh]);
  const exportCsv = async () => {
    if (exportPending.current) return;
    exportPending.current = true;
    setExporting(true);
    setNotice(null);
    setError(null);
    try {
      const data = await invoke<HistoryRecord[]>('get_history', { resolution, limit: resolution === 'minute' ? 10080 : 366 });
      const filename = 'NetDisk_History_' + resolution + '_' + new Date().toISOString().slice(0, 10) + '.csv';
      const path = await invoke<string>('export_csv_file', { filename, content: buildHistoryCsv(data, diskAvailable) });
      setNotice('Đã lưu CSV: ' + path);
    } catch (failure) { setError(errorMessage(failure)); }
    finally { exportPending.current = false; setExporting(false); }
  };
  const pages = Math.max(1, Math.ceil(records.length / 24));
  const currentPage = Math.min(page, pages - 1);
  return <section className="history-section">
    <div className="history-toolbar">
      <h3>Lịch sử tổng máy</h3>
      <select aria-label="Độ chi tiết lịch sử" value={resolution} onChange={event => setResolution(event.target.value as 'minute' | 'day')}>
        <option value="day">Ngày · 366 ngày gần nhất</option>
        <option value="minute">Phút · 120 phút gần nhất</option>
      </select>
      <button type="button" title="Làm mới lịch sử" aria-label="Làm mới lịch sử" disabled={loading} onClick={() => setRefresh(value => value + 1)}><RefreshCw size={15} /></button>
      <button type="button" title="Xuất toàn bộ lịch sử đã giữ" aria-label="Xuất lịch sử CSV" disabled={exporting || loading || !records.length} onClick={() => void exportCsv()}><Download size={15} /></button>
    </div>
    {error && <div className="diagnostics" role="alert">{error}</div>}
    {notice && <div className="notice" role="status">{notice}</div>}
    <div className="history-scroll" tabIndex={0} aria-label="Bảng lịch sử">
      <table><thead><tr><th>Thời điểm</th><th>Đã đo</th><th>Tải xuống</th><th>Tải lên</th><th>I/O đọc</th><th>I/O ghi</th></tr></thead>
        <tbody>{records.slice(currentPage * 24, (currentPage + 1) * 24).map(record => <tr key={record.timestampSecs}>
          <td>{new Date(record.timestampSecs * 1000).toLocaleString('vi-VN', resolution === 'day' ? { dateStyle: 'short' } : { dateStyle: 'short', timeStyle: 'short' })}</td>
          <td>{formatDuration(record.durationMillis / 1000)}</td>
          {record.bytes.map((bytes, index) => {
            const rate = historyRate(bytes, record.durationMillis);
            const available = index < 2 || diskAvailable;
            return <td key={index} title={available && rate !== null ? formatRate(rate) : undefined}>{available ? formatBytes(bytes) : 'N/A'}</td>;
          })}
        </tr>)}</tbody>
      </table>
    </div>
    {!records.length && <div className="empty">{loading ? 'Đang đọc lịch sử...' : 'Chưa có dữ liệu lịch sử.'}</div>}
    <div className="history-pagination">
      <span>{records.length} bản ghi</span>
      <button type="button" title="Trang lịch sử trước" aria-label="Trang lịch sử trước" disabled={currentPage === 0} onClick={() => setPage(currentPage - 1)}><ChevronLeft size={15} /></button>
      <span>{currentPage + 1} / {pages}</span>
      <button type="button" title="Trang lịch sử sau" aria-label="Trang lịch sử sau" disabled={currentPage >= pages - 1} onClick={() => setPage(currentPage + 1)}><ChevronRight size={15} /></button>
    </div>
  </section>;
}
