import { Cpu, HardDrive, Layers, Network, Timer, Wifi } from 'lucide-react';
import { formatBytes, formatDuration } from '../../lib/format';
import type { SystemSnapshot } from '../../types';

type SessionBandProps = {
  snapshot: SystemSnapshot;
  activeProcessCount: number;
};

export function SessionBand({ snapshot, activeProcessCount }: SessionBandProps) {
  const usedRam = snapshot.usedMemoryBytes;
  const sysRamText = snapshot.totalMemoryBytes ? ` / ${formatBytes(snapshot.totalMemoryBytes)}` : '';

  return (
    <section className="session-band">
      <div className="session-cell">
        <Network size={14} />
        <span className="session-label">Mạng phiên:</span>
        <span className="session-values">
          <b className="down">↓ {formatBytes(snapshot.sessionNetworkDown)}</b>
          <b className="up">↑ {formatBytes(snapshot.sessionNetworkUp)}</b>
        </span>
      </div>
      <div className="session-cell">
        <HardDrive size={14} />
        <span className="session-label">I/O phiên:</span>
        <span className="session-values">
          <b className="read">↓ {snapshot.diskAvailable ? formatBytes(snapshot.sessionDiskRead) : 'N/A'}</b>
          <b className="write">↑ {snapshot.diskAvailable ? formatBytes(snapshot.sessionDiskWrite) : 'N/A'}</b>
        </span>
      </div>
      <div className="session-cell">
        <Layers size={14} />
        <span className="session-label">Tiến trình:</span>
        <span className="session-values">
          <b className="duration">{activeProcessCount} đang chạy</b>
        </span>
      </div>
      <div className="session-cell">
        <Cpu size={14} />
        <span className="session-label">Tổng RAM:</span>
        <span className="session-values">
          <b className="duration">{formatBytes(usedRam)}{sysRamText}</b>
        </span>
      </div>
      <div className="session-cell">
        <Timer size={14} />
        <span className="session-label">Thời gian:</span>
        <span className="session-values">
          <b className="duration">{formatDuration(snapshot.sessionDurationSecs)}</b>
        </span>
      </div>
      <div className="session-cell">
        <Wifi size={14} />
        <span className="session-label">Card mạng:</span>
        <span className="session-values">
          <b className="duration">{snapshot.interfaces.length} giao diện</b>
        </span>
      </div>
    </section>
  );
}
