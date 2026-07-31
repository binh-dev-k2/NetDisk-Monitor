import { Cpu, HardDrive, Layers, Network, Timer, Wifi } from 'lucide-react';
import { formatBytes, formatDuration } from '../../lib/format';
import type { Snapshot } from '../../types';

type SessionBandProps = {
  snapshot: Snapshot;
};

export function SessionBand({ snapshot }: SessionBandProps) {
  const totalRamBytes = snapshot.processes.reduce((acc, p) => acc + p.memoryBytes, 0);
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
        <span className="session-label">Đĩa phiên:</span>
        <span className="session-values">
          <b className="read">↓ {formatBytes(snapshot.sessionDiskRead)}</b>
          <b className="write">↑ {formatBytes(snapshot.sessionDiskWrite)}</b>
        </span>
      </div>
      <div className="session-cell">
        <Layers size={14} />
        <span className="session-label">Tiến trình:</span>
        <span className="session-values">
          <b className="duration">{snapshot.processes.length} đang chạy</b>
        </span>
      </div>
      <div className="session-cell">
        <Cpu size={14} />
        <span className="session-label">Tổng RAM:</span>
        <span className="session-values">
          <b className="duration">{formatBytes(totalRamBytes)}{sysRamText}</b>
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
          <b className="duration">{snapshot.interfaces.length ? 'Hoạt động' : 'Tự động'}</b>
        </span>
      </div>
    </section>
  );
}
