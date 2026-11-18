import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { defaultOverlaySettings, normalizeOverlaySettings, type OverlaySettings } from '../lib/overlay-settings';
import { errorMessage } from '../lib/tauri-stream';
import { useTauriStream } from './useTauriStream';
import type { ThemeMode } from '../types';

export function useOverlaySettings() {
  const [settings, setSettings, settingsStatus] = useTauriStream<OverlaySettings>('get_overlay_settings', 'overlay://settings', {
    defaultValue: defaultOverlaySettings, normalize: normalizeOverlaySettings,
  });
  const [taskbarMode, setTaskbarMode, modeStatus] = useTauriStream<boolean>('get_taskbar_mode', 'taskbar://mode', { defaultValue: false });
  const [overlayVisible, setOverlayVisible, visibilityStatus] = useTauriStream<boolean>('get_overlay_visible', 'overlay://visibility', { defaultValue: true });
  const [overlayPinned, setOverlayPinned, pinStatus] = useTauriStream<boolean>('get_overlay_pinned', 'overlay://pinned', { defaultValue: true });
  const [theme, setTheme, themeStatus] = useTauriStream<ThemeMode>('get_theme', 'app://theme', { defaultValue: 'system' });
  const [draft, setDraft] = useState<OverlaySettings | null>(null);
  const draftRef = useRef<OverlaySettings | null>(null);
  const settingsRef = useRef(settings);
  settingsRef.current = settings;
  const settingsPending = useRef(false);
  const overlayPending = useRef(false);
  const taskbarPending = useRef(false);
  const pinPending = useRef(false);
  const autostartPending = useRef(false);
  const [autostart, setAutostart] = useState(false);
  const [isPendingOverlay, setIsPendingOverlay] = useState(false);
  const [isPendingTaskbar, setIsPendingTaskbar] = useState(false);
  const [isPendingAutostart, setIsPendingAutostart] = useState(false);
  const [commandError, setCommandError] = useState<string | null>(null);

  useEffect(() => {
    if (typeof document === 'undefined') return;
    if (theme === 'system') delete document.documentElement.dataset.theme;
    else document.documentElement.dataset.theme = theme;
  }, [theme]);

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

  const toggleOverlayVisible = async (visible: boolean) => {
    if (overlayPending.current) return;
    overlayPending.current = true;
    setIsPendingOverlay(true);
    setCommandError(null);
    try {
      setOverlayVisible(await invoke<boolean>('set_overlay_visible', { visible }));
    } catch (error) {
      setCommandError(errorMessage(error));
      try { setOverlayVisible(await invoke<boolean>('get_overlay_visible')); } catch { /* Keep original error */ }
    } finally { overlayPending.current = false; setIsPendingOverlay(false); }
  };

  const toggleTaskbarMode = async (enabled: boolean) => {
    if (taskbarPending.current) return;
    taskbarPending.current = true;
    setIsPendingTaskbar(true);
    setCommandError(null);
    try {
      setTaskbarMode(await invoke<boolean>('set_taskbar_mode', { enabled }));
    } catch (error) {
      setCommandError(errorMessage(error));
      try { setTaskbarMode(await invoke<boolean>('get_taskbar_mode')); } catch { /* Keep original error */ }
    } finally { taskbarPending.current = false; setIsPendingTaskbar(false); }
  };

  const toggleOverlayPinned = async (pinned: boolean) => {
    if (pinPending.current) return;
    pinPending.current = true;
    setCommandError(null);
    try {
      await invoke('set_overlay_pinned', { pinned });
      setOverlayPinned(pinned);
    } catch (error) {
      setCommandError(errorMessage(error));
    } finally { pinPending.current = false; }
  };

  const resetOverlayPosition = async () => {
    setCommandError(null);
    try { await invoke('reset_overlay_position'); }
    catch (error) { setCommandError(errorMessage(error)); }
  };

  const changeTheme = async (next: ThemeMode) => {
    setCommandError(null);
    try { setTheme(await invoke<ThemeMode>('set_theme', { theme: next })); }
    catch (error) { setCommandError(errorMessage(error)); }
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

  return {
    overlaySettings: draft ?? settings, taskbarMode, overlayVisible, overlayPinned, theme, autostart,
    isPendingOverlay, isPendingTaskbar, isPendingAutostart,
    error: commandError ?? settingsStatus.error ?? modeStatus.error ?? visibilityStatus.error ?? pinStatus.error ?? themeStatus.error,
    updateOverlaySettings, toggleOverlayVisible, toggleTaskbarMode, toggleOverlayPinned,
    resetOverlayPosition, changeTheme, toggleAutostart,
  };
}
