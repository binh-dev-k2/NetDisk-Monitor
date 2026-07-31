import { ArrowDown, ArrowUp, Database, HardDrive } from 'lucide-react';
import { MetricCard } from './MetricCard';
import { SessionBand } from './overview/SessionBand';
import { SettingsPanel } from './overview/SettingsPanel';
import { formatRate } from '../lib/format';
import type { OverlaySettings } from '../lib/overlay-settings';
import type { History, Snapshot } from '../types';

type OverviewTabProps = {
  snapshot: Snapshot;
  history: History;
  overlayVisible: boolean;
  taskbarMode: boolean;
  isPendingToggle: boolean;
  overlaySettings: OverlaySettings;
  autostart: boolean;
  handleDisplayModeChange: (mode: 'overlay' | 'taskbar' | 'hidden') => Promise<void>;
  updateOverlaySettings: (change: Partial<OverlaySettings>) => void;
  toggleAutostart: (enabled: boolean) => Promise<void>;
  reset: () => void;
};

export function OverviewTab({
  snapshot,
  history,
  overlayVisible,
  taskbarMode,
  isPendingToggle,
  overlaySettings,
  autostart,
  handleDisplayModeChange,
  updateOverlaySettings,
  toggleAutostart,
  reset,
}: OverviewTabProps) {
  return (
    <div className="overview-content">
      <section className="metric-grid">
        <MetricCard
          icon={<ArrowDown size={18} />}
          label="Tải xuống"
          value={formatRate(snapshot.networkDownBps)}
          tone="down"
          history={history.down}
        />
        <MetricCard
          icon={<ArrowUp size={18} />}
          label="Tải lên"
          value={formatRate(snapshot.networkUpBps)}
          tone="up"
          history={history.up}
        />
        <MetricCard
          icon={<HardDrive size={18} />}
          label="Đọc ổ đĩa"
          value={formatRate(snapshot.diskReadBps)}
          tone="read"
          history={history.read}
        />
        <MetricCard
          icon={<Database size={18} />}
          label="Ghi ổ đĩa"
          value={formatRate(snapshot.diskWriteBps)}
          tone="write"
          history={history.write}
        />
      </section>

      <SessionBand snapshot={snapshot} />

      <SettingsPanel
        overlayVisible={overlayVisible}
        taskbarMode={taskbarMode}
        isPendingToggle={isPendingToggle}
        overlaySettings={overlaySettings}
        autostart={autostart}
        handleDisplayModeChange={handleDisplayModeChange}
        updateOverlaySettings={updateOverlaySettings}
        toggleAutostart={toggleAutostart}
        reset={reset}
      />
    </div>
  );
}
