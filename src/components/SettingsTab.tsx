import { RotateCcw } from 'lucide-react';
import { DiagnosticsPanel } from './DiagnosticsPanel';
import { StoragePanel } from './StoragePanel';
import type { OverlaySettings } from '../lib/overlay-settings';
import type { SystemSnapshot, TaskbarPhase, ThemeMode } from '../types';

export function taskbarPhaseLabel(phase?: TaskbarPhase, enabled?: boolean): string {
  switch (phase) {
    case 'attached': return 'Đã gắn';
    case 'starting': return 'Đang khởi tạo';
    case 'degraded': return 'Bị giới hạn';
    case 'unsupported': return 'Không hỗ trợ';
    case 'disabled': return 'Đang tắt';
    default: return enabled ? 'Đang khởi tạo' : 'Đang tắt';
  }
}

type SettingsTabProps = {
  active: boolean;
  snapshot: SystemSnapshot;
  overlayVisible: boolean;
  overlayPinned: boolean;
  taskbarMode: boolean;
  taskbarError?: string | null;
  isPendingOverlay: boolean;
  isPendingTaskbar: boolean;
  overlaySettings: OverlaySettings;
  theme: ThemeMode;
  autostart: boolean;
  isPendingAutostart: boolean;
  resetPending: boolean;
  toggleOverlayVisible: (visible: boolean) => Promise<void>;
  toggleOverlayPinned: (pinned: boolean) => Promise<void>;
  resetOverlayPosition: () => Promise<void>;
  toggleTaskbarMode: (enabled: boolean) => Promise<void>;
  updateOverlaySettings: (change: Partial<OverlaySettings>) => void;
  changeTheme: (theme: ThemeMode) => Promise<void>;
  toggleAutostart: (enabled: boolean) => Promise<void>;
  reset: () => void;
};

const METRIC_OPTIONS = [
  { key: 'showNetworkDown', label: 'Tốc độ tải xuống' },
  { key: 'showNetworkUp', label: 'Tốc độ tải lên' },
  { key: 'showDiskRead', label: 'Tốc độ I/O đọc' },
  { key: 'showDiskWrite', label: 'Tốc độ I/O ghi' },
  { key: 'showRam', label: 'Bộ nhớ RAM' },
  { key: 'showSessionTotal', label: 'Tổng mạng phiên' },
  { key: 'showSessionDuration', label: 'Thời gian hoạt động' },
] as const;

export function SettingsTab({
  active, snapshot, overlayVisible, overlayPinned, taskbarMode, taskbarError,
  isPendingOverlay, isPendingTaskbar, overlaySettings, theme, autostart,
  isPendingAutostart, resetPending, toggleOverlayVisible, toggleOverlayPinned,
  resetOverlayPosition, toggleTaskbarMode, updateOverlaySettings, changeTheme,
  toggleAutostart, reset,
}: SettingsTabProps) {
  const isMac = typeof navigator !== 'undefined' && /(Macintosh|Mac OS X)/.test(navigator.userAgent);
  const phase = snapshot.taskbarStatus?.phase;
  const placementError = taskbarError ?? snapshot.taskbarStatus?.reason;

  return (
    <div className="settings-layout">
      <section className="settings-grid">
        <div className="settings-column main-controls">
          <h3>Hiển thị độc lập</h3>
          <label className="toggle-switch">
            <input type="checkbox" aria-label="Thanh nổi màn hình" checked={overlayVisible} disabled={isPendingOverlay} onChange={e => void toggleOverlayVisible(e.target.checked)} />
            <span>Thanh nổi màn hình (Overlay)</span>
          </label>
          <label className="toggle-switch">
            <input type="checkbox" aria-label="Ghim thanh nổi trên cùng" checked={overlayPinned} onChange={e => void toggleOverlayPinned(e.target.checked)} />
            <span>Ghim thanh nổi trên cùng</span>
          </label>
          <div className="control-field opacity-field">
            <div className="opacity-header">
              <span>Độ mờ thanh nổi</span>
              <output>{overlaySettings.opacity}%</output>
            </div>
            <div className="opacity-slider-container">
              <input type="range" min="35" max="92" value={overlaySettings.opacity} aria-label="Độ mờ thanh nổi" disabled={!overlayVisible} onChange={e => updateOverlaySettings({ opacity: Number(e.target.value) })} />
            </div>
          </div>
          <button type="button" className="icon-button-text" onClick={() => void resetOverlayPosition()}>
            Đặt lại vị trí thanh nổi
          </button>

          <div className="presenter-divider" />
          <div className="taskbar-toggle-row">
            <label className="toggle-switch">
              <input type="checkbox" aria-label="Hiển thị trên Taskbar" checked={taskbarMode} disabled={isPendingTaskbar} onChange={e => void toggleTaskbarMode(e.target.checked)} />
              <span>{isMac ? 'Hiện trên menu bar' : 'Ghim vào Taskbar'}</span>
            </label>
            <span className={`status-badge ${phase === 'attached' ? 'ok' : taskbarMode ? 'warn' : 'muted'}`}>
              {taskbarPhaseLabel(phase, taskbarMode)}
            </span>
          </div>
          {taskbarMode && placementError && <div className="diagnostics" role="alert">{placementError}</div>}
        </div>

        <div className="settings-column main-controls">
          <h3>Hệ thống &amp; Giao diện</h3>
          <div className="control-field">
            <label htmlFor="theme-select" className="field-label">Giao diện</label>
            <select id="theme-select" className="widget-mode-select" aria-label="Giao diện" value={theme} onChange={e => void changeTheme(e.target.value as ThemeMode)}>
              <option value="system">Theo hệ thống</option>
              <option value="dark">Tối (Dark)</option>
              <option value="light">Sáng (Light)</option>
            </select>
          </div>
          <label className="toggle-switch autostart-toggle">
            <input type="checkbox" checked={autostart} disabled={isPendingAutostart} onChange={e => void toggleAutostart(e.target.checked)} />
            <span>Khởi động cùng hệ thống (chạy nền vào khay)</span>
          </label>
          <button type="button" className="reset-button" onClick={reset} disabled={resetPending}>
            <RotateCcw size={14} aria-hidden="true" /> Reset Phiên
          </button>
        </div>

        <div className="settings-column checkbox-controls">
          <h3>Thông số hiển thị trên Widget</h3>
          <div className="checkbox-grid">
            {METRIC_OPTIONS.map(option => (
              <label className="toggle-check" key={option.key}>
                <input type="checkbox" checked={overlaySettings[option.key]} onChange={e => updateOverlaySettings({ [option.key]: e.target.checked })} />
                <span>{option.label}</span>
              </label>
            ))}
          </div>
        </div>
      </section>

      <StoragePanel active={active} />
      <DiagnosticsPanel active={active} snapshot={snapshot} />
    </div>
  );
}
