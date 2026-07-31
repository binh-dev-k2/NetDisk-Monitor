export type OverlaySettings = {
  showNetworkDown: boolean;
  showNetworkUp: boolean;
  showDiskRead: boolean;
  showDiskWrite: boolean;
  showSessionTotal: boolean;
  showSessionDuration: boolean;
  showRam: boolean;
  opacity: number;
};

export const defaultOverlaySettings: OverlaySettings = {
  showNetworkDown: true,
  showNetworkUp: true,
  showDiskRead: false,
  showDiskWrite: false,
  showSessionTotal: false,
  showSessionDuration: false,
  showRam: true,
  opacity: 72,
};

export function normalizeOverlaySettings(settings: Partial<OverlaySettings>): OverlaySettings {
  return {
    ...defaultOverlaySettings,
    ...settings,
    opacity: Math.min(92, Math.max(35, Math.round(settings.opacity ?? defaultOverlaySettings.opacity))),
  };
}
