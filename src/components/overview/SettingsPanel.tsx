import { RotateCcw } from 'lucide-react';
import type { OverlaySettings } from '../../lib/overlay-settings';

type SettingsPanelProps = {
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

export function SettingsPanel({
  overlayVisible,
  taskbarMode,
  isPendingToggle,
  overlaySettings,
  autostart,
  handleDisplayModeChange,
  updateOverlaySettings,
  toggleAutostart,
  reset,
}: SettingsPanelProps) {
  return (
    <section className="settings-panel">
      <div className="settings-column main-controls">
        <h3>CHẾ ĐỘ & ĐỘ MỜ WIDGET</h3>
        <div className="control-field">
          <select
            className="widget-mode-select"
            value={!overlayVisible ? 'hidden' : taskbarMode ? 'taskbar' : 'overlay'}
            disabled={isPendingToggle}
            onChange={e => handleDisplayModeChange(e.target.value as 'overlay' | 'taskbar' | 'hidden')}
          >
            <option value="overlay">Thanh nổi màn hình</option>
            <option value="taskbar">Ghim vào Taskbar</option>
            <option value="hidden">Ẩn Widget</option>
          </select>
        </div>

        <div className="control-field opacity-field">
          <div className="opacity-header">
            <span>Độ mờ Widget</span>
            <output>{overlaySettings.opacity}%</output>
          </div>
          <div className="opacity-slider-container">
            <input
              type="range"
              min="35"
              max="92"
              value={overlaySettings.opacity}
              onChange={event => updateOverlaySettings({ opacity: Number(event.target.value) })}
            />
          </div>
        </div>

        <label className="toggle-check autostart-toggle">
          <input
            type="checkbox"
            checked={autostart}
            onChange={e => toggleAutostart(e.target.checked)}
          />
          <span>Khởi động cùng hệ thống</span>
        </label>

        <button className="reset-button" onClick={reset}>
          <RotateCcw size={14} /> Reset Phiên
        </button>
      </div>

      <div className="settings-column checkbox-controls">
        <h3>THÔNG SỐ HIỂN THỊ TRÊN WIDGET</h3>
        <div className="checkbox-grid">
          {([
            { key: 'showNetworkDown', label: 'Tốc độ tải xuống' },
            { key: 'showNetworkUp', label: 'Tốc độ tải lên' },
            { key: 'showDiskRead', label: 'Tốc độ đọc đĩa' },
            { key: 'showDiskWrite', label: 'Tốc độ ghi đĩa' },
            { key: 'showRam', label: 'Bộ nhớ RAM' },
            { key: 'showSessionTotal', label: 'Tổng dung lượng phiên' },
            { key: 'showSessionDuration', label: 'Thời gian hoạt động' },
          ] as const).map(option => (
            <label className="toggle-check" key={option.key}>
              <input
                type="checkbox"
                checked={overlaySettings[option.key]}
                onChange={event => updateOverlaySettings({ [option.key]: event.target.checked })}
              />
              <span>{option.label}</span>
            </label>
          ))}
        </div>
      </div>
    </section>
  );
}
