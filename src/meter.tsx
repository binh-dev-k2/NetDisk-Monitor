import { StrictMode, useEffect, useMemo, useRef, useState, type CSSProperties, type MouseEvent, type ReactElement } from 'react';
import { createRoot } from 'react-dom/client';
import { invoke } from '@tauri-apps/api/core';
import { ArrowDown, ArrowUp, Clock3, Cpu, Database, HardDrive, Pin, PinOff, Sigma, X } from 'lucide-react';
import { formatBytes, formatDuration, formatRate } from './lib/format';
import { overlayLayout } from './lib/overlay-layout';
import { defaultOverlaySettings, normalizeOverlaySettings, type OverlaySettings } from './lib/overlay-settings';
import { errorMessage } from './lib/tauri-stream';
import { useTauriStream } from './hooks/useTauriStream';
import { emptySnapshot, snapshotSequence, systemSnapshot } from './hooks/useMonitorSnapshot';
import type { SystemSnapshot } from './types';
import './meter.css';

type MeterItem = { label: string; value: string; tone: string; icon: ReactElement };

function Meter() {
  const [inTaskbar, , modeStatus] = useTauriStream<boolean>('get_taskbar_mode', 'taskbar://mode', { defaultValue: false });
  const [visible] = useTauriStream<boolean>('get_overlay_visible', 'overlay://visibility', { defaultValue: true });
  const [snapshot, , snapshotStatus] = useTauriStream<SystemSnapshot>('get_snapshot', 'metrics://snapshot', {
    defaultValue: emptySnapshot, normalize: systemSnapshot, sequence: snapshotSequence,
    transition: true, pollInterval: 1000, staleAfter: 3500, enabled: !inTaskbar && visible,
  });
  const [settings, , settingsStatus] = useTauriStream<OverlaySettings>('get_overlay_settings', 'overlay://settings', {
    defaultValue: defaultOverlaySettings, normalize: normalizeOverlaySettings,
  });
  const [pinned, setPinned, pinStatus] = useTauriStream<boolean>('get_overlay_pinned', 'overlay://pinned', { defaultValue: true });
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const pendingRef = useRef(false);
  const items = useMemo(() => [
    settings.showNetworkDown && { label: 'Tải xuống', value: formatRate(snapshot.networkDownBps), tone: 'down', icon: <ArrowDown size={15} /> },
    settings.showNetworkUp && { label: 'Tải lên', value: formatRate(snapshot.networkUpBps), tone: 'up', icon: <ArrowUp size={15} /> },
    settings.showDiskRead && { label: 'I/O đọc', value: snapshot.diskAvailable ? formatRate(snapshot.diskReadBps) : 'N/A', tone: 'read', icon: <HardDrive size={15} /> },
    settings.showDiskWrite && { label: 'I/O ghi', value: snapshot.diskAvailable ? formatRate(snapshot.diskWriteBps) : 'N/A', tone: 'write', icon: <Database size={15} /> },
    settings.showRam && { label: 'Bộ nhớ RAM', value: formatBytes(snapshot.usedMemoryBytes), tone: 'ram', icon: <Cpu size={15} /> },
    settings.showSessionTotal && { label: 'Mạng phiên', value: formatBytes(snapshot.sessionNetworkDown + snapshot.sessionNetworkUp), tone: 'total', icon: <Sigma size={15} /> },
    settings.showSessionDuration && { label: 'Thời gian phiên', value: formatDuration(snapshot.sessionDurationSecs), tone: 'duration', icon: <Clock3 size={15} /> },
  ].filter((item): item is MeterItem => Boolean(item)), [settings, snapshot]);
  const diagnostic = [error, snapshot.storageError, snapshot.taskbarError, snapshotStatus.error, modeStatus.error, settingsStatus.error, pinStatus.error].filter(Boolean).join(' | ') || null;
  const showStatus = Boolean(diagnostic || snapshotStatus.stale || snapshotStatus.loading);
  const layout = overlayLayout(items.length);
  const height = Math.min(164, layout.height + (showStatus ? 24 : 0));
  useEffect(() => {
    if (inTaskbar || !visible || modeStatus.loading) return;
    let disposed = false;
    void invoke('resize_overlay', { width: layout.width, height })
      .catch(failure => { if (!disposed) setError(errorMessage(failure)); });
    return () => { disposed = true; };
  }, [height, layout.width, inTaskbar, visible, modeStatus.loading]);
  const command = async (action: () => Promise<void>) => {
    if (pendingRef.current) return;
    pendingRef.current = true;
    setPending(true);
    setError(null);
    try { await action(); }
    catch (failure) { setError(errorMessage(failure)); }
    finally { pendingRef.current = false; setPending(false); }
  };
  const togglePin = (event: MouseEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    void command(async () => { await invoke('set_overlay_pinned', { pinned: !pinned }); setPinned(!pinned); });
  };
  const closeOverlay = (event: MouseEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    void command(async () => { await invoke('set_overlay_visible', { visible: false }); });
  };
  if (inTaskbar || !visible) return null;
  const style = { '--meter-opacity': String(settings.opacity / 100), '--meter-columns': String(layout.columns), '--meter-rows': String(layout.rows) } as CSSProperties;
  return <div className="meter-shell" style={style}>
    <div className="meter" aria-label="Network and I/O monitor" onPointerDown={event => {
      if (event.button !== 0 || (event.target as HTMLElement).closest('button')) return;
      void invoke('begin_overlay_drag').catch(failure => setError(errorMessage(failure)));
    }}>
      <div className="meter-controls" onPointerDown={event => event.stopPropagation()}>
        <button className={`meter-control-btn ${pinned ? 'active' : ''}`} type="button" disabled={pending || pinStatus.loading}
          title={pinned ? 'Bỏ ghim trên cùng' : 'Ghim trên cùng'} aria-label={pinned ? 'Bỏ ghim trên cùng' : 'Ghim trên cùng'} onClick={togglePin}>
          {pinned ? <Pin size={11} /> : <PinOff size={11} />}
        </button>
        <button className="meter-control-btn" type="button" title="Ẩn widget" aria-label="Ẩn widget" disabled={pending} onClick={closeOverlay}><X size={11} /></button>
      </div>
      {items.length ? items.map(item => <div className={`meter-item ${item.tone}`} key={item.tone}>
        <span className="meter-icon">{item.icon}</span><span><small>{item.label}</small><b title={item.value}>{item.value}</b></span>
      </div>) : <div className="meter-empty">Không có chỉ số được chọn</div>}
    </div>
    {showStatus && <div className="meter-diagnostic" role={diagnostic ? 'alert' : 'status'} title={diagnostic ?? undefined}>
      {diagnostic ?? (snapshotStatus.stale ? 'Dữ liệu cũ' : 'Đang kết nối')}
    </div>}
  </div>;
}

createRoot(document.getElementById('root')!).render(<StrictMode><Meter /></StrictMode>);
