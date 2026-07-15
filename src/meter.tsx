import { StrictMode, useEffect, useMemo, useState, type CSSProperties, type MouseEvent, type PointerEvent, type ReactElement } from 'react';
import { createRoot } from 'react-dom/client';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { ArrowDown, ArrowUp, Clock3, Database, HardDrive, Pin, PinOff, Sigma, X } from 'lucide-react';
import { formatBytes, formatDuration, formatRate } from './lib/format';
import { overlayLayout } from './lib/overlay-layout';
import { taskbarLayout } from './lib/taskbar-layout';
import { defaultOverlaySettings, normalizeOverlaySettings, type OverlaySettings } from './lib/overlay-settings';
import './meter.css';

type Snapshot = { networkDownBps: number; networkUpBps: number; diskReadBps: number; diskWriteBps: number; sessionNetworkDown: number; sessionNetworkUp: number; sessionDiskRead: number; sessionDiskWrite: number; sessionDurationSecs: number };
type MeterItem = { label: string; value: string; tone: string; icon: ReactElement };
const empty: Snapshot = { networkDownBps: 0, networkUpBps: 0, diskReadBps: 0, diskWriteBps: 0, sessionNetworkDown: 0, sessionNetworkUp: 0, sessionDiskRead: 0, sessionDiskWrite: 0, sessionDurationSecs: 0 };

function Meter() {
  const [snapshot, setSnapshot] = useState<Snapshot>(empty);
  const [settings, setSettings] = useState<OverlaySettings>(defaultOverlaySettings);
  const [pinned, setPinned] = useState(true);
  const [inTaskbar, setInTaskbar] = useState(false);
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void invoke<Snapshot>('get_snapshot').then(setSnapshot).catch(console.error);
    void listen<Snapshot>('metrics://snapshot', event => setSnapshot(event.payload)).then(listener => { unlisten = listener; });
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void invoke<boolean>('get_taskbar_mode').then(setInTaskbar).catch(console.error);
    void listen<boolean>('taskbar://mode', event => setInTaskbar(event.payload)).then(listener => { unlisten = listener; });
    return () => unlisten?.();
  }, []);
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void invoke<OverlaySettings>('get_overlay_settings').then(value => setSettings(normalizeOverlaySettings(value))).catch(console.error);
    void listen<OverlaySettings>('overlay://settings', event => setSettings(normalizeOverlaySettings(event.payload))).then(listener => { unlisten = listener; });
    return () => unlisten?.();
  }, []);

  const items = useMemo(() => [
    settings.showNetworkDown && { label: 'Tải xuống', value: formatRate(snapshot.networkDownBps), tone: 'down', icon: <ArrowDown size={15} /> },
    settings.showNetworkUp && { label: 'Tải lên', value: formatRate(snapshot.networkUpBps), tone: 'up', icon: <ArrowUp size={15} /> },
    settings.showDiskRead && { label: 'Đọc ổ đĩa', value: formatRate(snapshot.diskReadBps), tone: 'read', icon: <HardDrive size={15} /> },
    settings.showDiskWrite && { label: 'Ghi ổ đĩa', value: formatRate(snapshot.diskWriteBps), tone: 'write', icon: <Database size={15} /> },
    settings.showSessionTotal && { label: 'Tổng phiên', value: formatBytes(snapshot.sessionNetworkDown + snapshot.sessionNetworkUp + snapshot.sessionDiskRead + snapshot.sessionDiskWrite), tone: 'total', icon: <Sigma size={15} /> },
    settings.showSessionDuration && { label: 'Thời gian phiên', value: formatDuration(snapshot.sessionDurationSecs), tone: 'duration', icon: <Clock3 size={15} /> },
  ].filter((item): item is MeterItem => Boolean(item)), [settings, snapshot]);
  const visibleItems = items;
  const layout = inTaskbar ? taskbarLayout(visibleItems.length) : overlayLayout(visibleItems.length);

  useEffect(() => {
    void invoke('resize_overlay', { width: layout.width, height: layout.height }).catch(console.error);
  }, [layout.height, layout.width]);

  const togglePin = (event: MouseEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    const next = !pinned;
    setPinned(next);
    void invoke('set_overlay_pinned', { pinned: next }).catch(console.error);
  };
  const closeOverlay = (event: MouseEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    void invoke('set_overlay_visible', { visible: false }).catch(console.error);
  };
  const style = {
    '--meter-opacity': String(settings.opacity / 100),
    '--meter-columns': String(layout.columns),
    '--meter-rows': String(layout.rows)
  } as CSSProperties;

  return (
    <div className={`meter ${inTaskbar ? 'in-taskbar' : ''}`} aria-label="Network and disk monitor" style={style}>
      {!inTaskbar && (
        <div className="meter-controls" onMouseDown={event => event.stopPropagation()} onPointerDown={event => event.stopPropagation()}>
          <button className={`meter-control-btn ${pinned ? 'active' : ''}`} type="button" title={pinned ? 'Bỏ ghim' : 'Ghim trên cùng'} onClick={togglePin}>
            {pinned ? <Pin size={11} /> : <PinOff size={11} />}
          </button>
          <button className="meter-control-btn" type="button" title="Đóng" onClick={closeOverlay}>
            <X size={11} />
          </button>
        </div>
      )}
      {visibleItems.length ? visibleItems.map(item => (
        <div className={`meter-item ${item.tone}`} key={item.label}>
          <span className="meter-icon">{item.icon}</span>
          <span>
            <small>{item.label}</small>
            <b>{item.value}</b>
          </span>
        </div>
      )) : (
        <div className="meter-empty">Chọn chỉ số trong dashboard</div>
      )}
    </div>
  );
}

createRoot(document.getElementById('root')!).render(<StrictMode><Meter /></StrictMode>);
