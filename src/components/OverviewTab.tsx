import { ArrowDown, ArrowUp, Database, HardDrive, RotateCcw, SlidersHorizontal } from 'lucide-react';
import { MetricCard } from './MetricCard';
import { SessionBand } from './overview/SessionBand';
import { taskbarPhaseLabel } from './SettingsTab';
import { formatBytes, formatRate } from '../lib/format';
import type { History, SystemSnapshot } from '../types';

type OverviewTabProps = {
  snapshot: SystemSnapshot;
  activeProcessCount: number | null;
  history: History;
  loading: boolean;
  error: string | null;
  overlayVisible: boolean;
  taskbarMode: boolean;
  resetPending: boolean;
  onRetry: () => void;
  onOpenDisk: () => void;
  onOpenSettings: () => void;
  reset: () => void;
};

export function OverviewTab({
  snapshot,
  activeProcessCount,
  history,
  loading,
  error,
  overlayVisible,
  taskbarMode,
  resetPending,
  onRetry,
  onOpenDisk,
  onOpenSettings,
  reset,
}: OverviewTabProps) {
  const taskbarLabel = taskbarPhaseLabel(snapshot.taskbarStatus?.phase, taskbarMode);
  const volumes = snapshot.volumes ?? [];
  return (
    <div className="overview-content">
      {error && (
        <div className="diagnostics diagnostics-row" role="alert">
          <span>{error}</span>
          <button type="button" className="inline-action-btn" onClick={onRetry}>Thử lại</button>
        </div>
      )}
      {loading && snapshot.sequence === 0 && !error && (
        <div className="notice" role="status">Đang kết nối bộ thu thập hệ thống...</div>
      )}
      <section className="metric-grid">
        <MetricCard icon={<ArrowDown size={18} />} label="Tải xuống" value={formatRate(snapshot.networkDownBps)} tone="down" history={history.down} />
        <MetricCard icon={<ArrowUp size={18} />} label="Tải lên" value={formatRate(snapshot.networkUpBps)} tone="up" history={history.up} />
        <MetricCard icon={<HardDrive size={18} />} label="I/O đọc" value={snapshot.diskAvailable ? formatRate(snapshot.diskReadBps) : 'N/A'} tone="read" history={history.read} />
        <MetricCard icon={<Database size={18} />} label="I/O ghi" value={snapshot.diskAvailable ? formatRate(snapshot.diskWriteBps) : 'N/A'} tone="write" history={history.write} />
      </section>

      <div className="overview-bento">
        <SessionBand snapshot={snapshot} activeProcessCount={activeProcessCount} />

        <section className="bento-card overview-side-panel">
          <div className="overview-interfaces">
            <div className="bento-card-header">
              <h3>Giao diện mạng đang theo dõi</h3>
              <span className="status-badge muted">{snapshot.interfaces.length} cổng</span>
            </div>
            {snapshot.interfaces.length > 0 ? (
              <div className="interface-list">
                {snapshot.interfaces.map(name => (
                  <span className="interface-pill" key={name}><i className="online" aria-hidden="true" />{name}</span>
                ))}
              </div>
            ) : (
              <p className="empty-inline">Chưa phát hiện giao diện mạng đang hoạt động.</p>
            )}
            {volumes.length > 0 && (
              <div className="overview-volumes">
                <div className="bento-card-header">
                  <h3>Dung lượng phân vùng</h3>
                  <button type="button" className="inline-action-btn" onClick={onOpenDisk}>Phân tích &amp; Dọn đĩa</button>
                </div>
                <div className="volume-list">
                  {volumes.slice(0, 4).map(vol => {
                    const pct = vol.totalBytes > 0 ? Math.min(100, Math.round((vol.usedBytes / vol.totalBytes) * 100)) : 0;
                    const tone = pct >= 90 ? 'err' : pct >= 80 ? 'warn' : 'ok';
                    return (
                      <div className="volume-row" key={vol.mountPoint}>
                        <div className="ram-meter-row">
                          <span><b>{vol.mountPoint}</b> {vol.name && `(${vol.name})`}</span>
                          <span className={`status-badge ${tone}`}>{pct}% · trống {formatBytes(vol.availableBytes)}</span>
                        </div>
                        <div className="meter-track" aria-hidden="true">
                          <div className="meter-fill" style={{ inlineSize: `${pct}%` }} />
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            )}
          </div>
          <div className="overview-quick-actions">
            <div className="presenter-pills">
              <span className={`status-badge ${overlayVisible ? 'ok' : 'muted'}`}>
                Thanh nổi: {overlayVisible ? 'Đang hiện' : 'Đang ẩn'}
              </span>
              <span className={`status-badge ${snapshot.taskbarStatus?.phase === 'attached' ? 'ok' : taskbarMode ? 'warn' : 'muted'}`}>
                Taskbar: {taskbarLabel}
              </span>
            </div>
            <div className="overview-buttons">
              <button type="button" className="icon-button-text" onClick={onOpenSettings}>
                <SlidersHorizontal size={14} aria-hidden="true" /> Cài đặt hiển thị
              </button>
              <button type="button" className="reset-button compact" onClick={reset} disabled={resetPending}>
                <RotateCcw size={14} aria-hidden="true" /> Reset Phiên
              </button>
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}
