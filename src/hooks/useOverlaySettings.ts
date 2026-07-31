import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { defaultOverlaySettings, normalizeOverlaySettings, type OverlaySettings } from '../lib/overlay-settings';

export function useOverlaySettings() {
  const [overlaySettings, setOverlaySettings] = useState<OverlaySettings>(defaultOverlaySettings);
  const [taskbarMode, setTaskbarMode] = useState(false);
  const [overlayVisible, setOverlayVisible] = useState(true);
  const [autostart, setAutostart] = useState(false);
  const [isPendingToggle, setIsPendingToggle] = useState(false);

  useEffect(() => {
    void invoke<OverlaySettings>('get_overlay_settings')
      .then(value => setOverlaySettings(normalizeOverlaySettings(value)))
      .catch(console.error);
    void invoke<boolean>('get_taskbar_mode').then(setTaskbarMode).catch(console.error);
    void invoke<boolean>('get_overlay_visible').then(setOverlayVisible).catch(console.error);
    void invoke<boolean>('get_autostart').then(setAutostart).catch(console.error);

    const unlistenSettings = listen<OverlaySettings>('overlay://settings-updated', event => {
      setOverlaySettings(normalizeOverlaySettings(event.payload));
    });

    const unlistenTaskbar = listen<boolean>('taskbar://mode-updated', event => {
      setTaskbarMode(event.payload);
    });

    const unlistenVisibility = listen<boolean>('overlay://visibility-updated', event => {
      setOverlayVisible(event.payload);
    });

    return () => {
      void unlistenSettings.then(f => f());
      void unlistenTaskbar.then(f => f());
      void unlistenVisibility.then(f => f());
    };
  }, []);

  const updateOverlaySettings = (change: Partial<OverlaySettings>) => {
    const next = normalizeOverlaySettings({ ...overlaySettings, ...change });
    setOverlaySettings(next);
    void invoke<OverlaySettings>('update_overlay_settings', { settings: next })
      .then(value => setOverlaySettings(normalizeOverlaySettings(value)))
      .catch(console.error);
  };

  const handleDisplayModeChange = async (mode: 'overlay' | 'taskbar' | 'hidden') => {
    if (isPendingToggle) return;
    setIsPendingToggle(true);
    try {
      if (mode === 'hidden') {
        await invoke('set_taskbar_mode', { enabled: false });
        await invoke('set_overlay_visible', { visible: false });
        setTaskbarMode(false);
        setOverlayVisible(false);
      } else if (mode === 'overlay') {
        await invoke('set_taskbar_mode', { enabled: false });
        await invoke('set_overlay_visible', { visible: true });
        setTaskbarMode(false);
        setOverlayVisible(true);
      } else if (mode === 'taskbar') {
        const resMode = await invoke<boolean>('set_taskbar_mode', { enabled: true });
        setTaskbarMode(resMode);
        setOverlayVisible(resMode);
      }
    } catch (err) {
      console.error(err);
    } finally {
      setIsPendingToggle(false);
    }
  };

  const toggleAutostart = async (enabled: boolean) => {
    try {
      const res = await invoke<boolean>('set_autostart', { enabled });
      setAutostart(res);
    } catch (err: any) {
      alert(err || 'Không thể thay đổi cài đặt khởi động cùng Windows');
    }
  };

  return {
    overlaySettings,
    taskbarMode,
    overlayVisible,
    autostart,
    isPendingToggle,
    updateOverlaySettings,
    handleDisplayModeChange,
    toggleAutostart,
  };
}
