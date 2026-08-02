import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { defaultOverlaySettings, normalizeOverlaySettings, type OverlaySettings } from '../lib/overlay-settings';
import { useTauriStream } from './useTauriStream';

export function useOverlaySettings() {
  const [overlaySettings, setOverlaySettings] = useTauriStream<OverlaySettings>(
    'get_overlay_settings',
    'overlay://settings',
    { defaultValue: defaultOverlaySettings, normalize: normalizeOverlaySettings },
  );
  const [taskbarMode, setTaskbarMode] = useTauriStream<boolean>('get_taskbar_mode', 'taskbar://mode', {
    defaultValue: false,
  });
  const [overlayVisible, setOverlayVisible] = useTauriStream<boolean>(
    'get_overlay_visible',
    'overlay://visibility',
    { defaultValue: true },
  );
  const [autostart, setAutostart] = useState(false);
  const [isPendingToggle, setIsPendingToggle] = useState(false);

  useEffect(() => {
    void invoke<boolean>('get_autostart').then(setAutostart).catch(console.error);
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
