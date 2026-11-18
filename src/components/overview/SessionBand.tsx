import { Cpu, HardDrive, Layers, Network, Timer, Wifi } from 'lucide-react';
import { formatBytes, formatDuration } from '../../lib/format';
import type { SystemSnapshot } from '../../types';

type SessionBandProps = {
  snapshot: SystemSnapshot;
  activeProcessCount: number | null;
};

export function SessionBand({ snapshot, activeProcessCount }: SessionBandProps) {
  const usedRam = snapshot.usedMemoryBytes;
  const totalRam = snapshot.totalMemoryBytes;
  const sysRamText = totalRam ? ` / ${formatBytes(totalRam)}` : '';
  const ramPercent = totalRam > 0 ? Math.min(100, Math.round((usedRam / totalRam) * 100)) : 0;

  return (
    <section className="bento-card session-panel">
      <div className="bento-card-header">
        <h3>Tổng kết phiên &amp; Tài nguyên</h3>
        <div className="presenter-pills">
          {snapshot.cpuUsagePercent !== undefined && (
            <span className="status-badge muted">CPU {Math.round(snapshot.cpuUsagePercent)}%</span>
          )}
          <span className="status-badge muted">{ramPercent}% RAM</span>
        </div>
      </div>
      <div className="ram-meter">
        <div className="ram-meter-row">
          <span>Bộ nhớ hệ thống đang dùng</span>
          <strong>{formatBytes(usedRam)}{sysRamText}</strong>
        </div>
        <div className="meter-track" role="progressbar" aria-valuenow={ramPercent} aria-valuemin={0} aria-valuemax={100} aria-label="Mức sử dụng RAM">
          <div className="meter-fill" style={{ inlineSize: `${ramPercent}%` }} />
        </div>
      </div>
      <div className="session-band">
        <div className="session-cell">
          <Network size={14} aria-hidden="true" />
          <span className="session-label">Mạng phiên:</span>
          <span className="session-values">
            <b className="down">↓ {formatBytes(snapshot.sessionNetworkDown)}</b>
            <b className="up">↑ {formatBytes(snapshot.sessionNetworkUp)}</b>
          </span>
        </div>
        <div className="session-cell">
          <HardDrive size={14} aria-hidden="true" />
          <span className="session-label">I/O phiên:</span>
          <span className="session-values">
            <b className="read">↓ {snapshot.diskAvailable ? formatBytes(snapshot.sessionDiskRead) : 'N/A'}</b>
            <b className="write">↑ {snapshot.diskAvailable ? formatBytes(snapshot.sessionDiskWrite) : 'N/A'}</b>
          </span>
        </div>
        <div className="session-cell">
          <Layers size={14} aria-hidden="true" />
          <span className="session-label">Tiến trình:</span>
          <span className="session-values">
            <b className="duration">{activeProcessCount === null ? 'N/A' : `${activeProcessCount} đang chạy`}</b>
          </span>
        </div>
        <div className="session-cell">
          <Cpu size={14} aria-hidden="true" />
          <span className="session-label">Tổng RAM:</span>
          <span className="session-values">
            <b className="duration">{formatBytes(usedRam)}{sysRamText}</b>
          </span>
        </div>
        <div className="session-cell">
          <Timer size={14} aria-hidden="true" />
          <span className="session-label">Thời gian:</span>
          <span className="session-values">
            <b className="duration">{formatDuration(snapshot.sessionDurationSecs)}</b>
          </span>
        </div>
        <div className="session-cell">
          <Wifi size={14} aria-hidden="true" />
          <span className="session-label">Card mạng:</span>
          <span className="session-values">
            <b className="duration">{snapshot.interfaces.length} giao diện</b>
          </span>
        </div>
      </div>
    </section>
  );
}
