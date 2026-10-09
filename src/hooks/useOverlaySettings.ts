import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { defaultOverlaySettings, normalizeOverlaySettings, type OverlaySettings } from '../lib/overlay-settings';
import { errorMessage } from '../lib/tauri-stream';
import { useTauriStream } from './useTauriStream';

export function useOverlaySettings() {
  const [settings, setSettings, settingsStatus] = useTauriStream<OverlaySettings>('get_overlay_settings', 'overlay://settings', {
    defaultValue: defaultOverlaySettings, normalize: normalizeOverlaySettings,
  });
  const [taskbarMode, setTaskbarMode, modeStatus] = useTauriStream<boolean>('get_taskbar_mode', 'taskbar://mode', { defaultValue: false });
  const [overlayVisible, setOverlayVisible, visibilityStatus] = useTauriStream<boolean>('get_overlay_visible', 'overlay://visibility', { defaultValue: true });
  const [draft, setDraft] = useState<OverlaySettings | null>(null);
  const draftRef = useRef<OverlaySettings | null>(null);
  const settingsRef = useRef(settings);
  settingsRef.current = settings;
  const settingsPending = useRef(false);
  const togglePending = useRef(false);
  const autostartPending = useRef(false);
  const [autostart, setAutostart] = useState(false);
  const [isPendingToggle, setIsPendingToggle] = useState(false);
  const [isPendingAutostart, setIsPendingAutostart] = useState(false);
  const [commandError, setCommandError] = useState<string | null>(null);
  useEffect(() => {
    let disposed = false;
    void invoke<boolean>('get_autostart').then(value => { if (!disposed) setAutostart(value); })
      .catch(error => { if (!disposed) setCommandError(errorMessage(error)); });
    return () => { disposed = true; };
  }, []);
  const updateOverlaySettings = (change: Partial<OverlaySettings>) => {
    const next = normalizeOverlaySettings({ ...(draftRef.current ?? settingsRef.current), ...change });
    draftRef.current = next;
    setDraft(next);
    if (settingsPending.current) return;
    settingsPending.current = true;
    setCommandError(null);
    void (async () => {
      try {
        while (draftRef.current) {
          const desired = draftRef.current;
          const value = normalizeOverlaySettings(await invoke<OverlaySettings>('update_overlay_settings', { settings: desired }));
          setSettings(value);
          if (draftRef.current === desired) { draftRef.current = null; setDraft(null); }
        }
      } catch (error) {
        draftRef.current = null;
        setDraft(null);
        setCommandError(errorMessage(error));
      } finally { settingsPending.current = false; }
    })();
  };
  const handleDisplayModeChange = async (mode: 'overlay' | 'taskbar' | 'hidden') => {
    if (togglePending.current) return;
    togglePending.current = true;
    setIsPendingToggle(true);
    setCommandError(null);
    try {
      if (mode === 'taskbar') {
        const enabled = await invoke<boolean>('set_taskbar_mode', { enabled: true });
        setTaskbarMode(enabled);
        setOverlayVisible(await invoke<boolean>('get_overlay_visible'));
      } else {
        await invoke('set_taskbar_mode', { enabled: false });
        setTaskbarMode(false);
        setOverlayVisible(await invoke<boolean>('set_overlay_visible', { visible: mode === 'overlay' }));
      }
    } catch (error) {
      setCommandError(errorMessage(error));
      try {
        setTaskbarMode(await invoke<boolean>('get_taskbar_mode'));
        setOverlayVisible(await invoke<boolean>('get_overlay_visible'));
      } catch { /* Preserve the original actionable error. */ }
    } finally { togglePending.current = false; setIsPendingToggle(false); }
  };
  const toggleAutostart = async (enabled: boolean) => {
    if (autostartPending.current) return;
    autostartPending.current = true;
    setIsPendingAutostart(true);
    setCommandError(null);
    try { setAutostart(await invoke<boolean>('set_autostart', { enabled })); }
    catch (error) { setCommandError(errorMessage(error)); }
    finally { autostartPending.current = false; setIsPendingAutostart(false); }
  };
  return { overlaySettings: draft ?? settings, taskbarMode, overlayVisible, autostart,
    isPendingToggle, isPendingAutostart,
    error: commandError ?? settingsStatus.error ?? modeStatus.error ?? visibilityStatus.error,
    updateOverlaySettings, handleDisplayModeChange, toggleAutostart };
}
