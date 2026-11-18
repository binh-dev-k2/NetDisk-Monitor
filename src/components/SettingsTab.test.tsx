import { describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { SettingsTab, taskbarPhaseLabel } from './SettingsTab';
import { defaultOverlaySettings } from '../lib/overlay-settings';
import type { SystemSnapshot } from '../types';

const baseSnapshot: SystemSnapshot = {
  sequence: 1,
  activeProcessCount: 12,
  diskAvailable: true,
  networkDownBps: 0,
  networkUpBps: 0,
  diskReadBps: 0,
  diskWriteBps: 0,
  sessionNetworkDown: 0,
  sessionNetworkUp: 0,
  sessionDiskRead: 0,
  sessionDiskWrite: 0,
  sessionDurationSecs: 10,
  interfaces: ['Ethernet'],
  totalMemoryBytes: 1024,
  usedMemoryBytes: 512,
};

function render(opts: { overlayVisible?: boolean; taskbarMode: boolean; taskbarError: string | null }) {
  return renderToStaticMarkup(
    <SettingsTab
      active={false}
      snapshot={baseSnapshot}
      overlayVisible={opts.overlayVisible ?? false}
      overlayPinned={true}
      taskbarMode={opts.taskbarMode}
      taskbarError={opts.taskbarError}
      isPendingOverlay={false}
      isPendingTaskbar={false}
      overlaySettings={defaultOverlaySettings}
      theme="system"
      autostart={false}
      isPendingAutostart={false}
      resetPending={false}
      toggleOverlayVisible={vi.fn(async () => {})}
      toggleOverlayPinned={vi.fn(async () => {})}
      resetOverlayPosition={vi.fn(async () => {})}
      toggleTaskbarMode={vi.fn(async () => {})}
      updateOverlaySettings={vi.fn()}
      changeTheme={vi.fn(async () => {})}
      toggleAutostart={vi.fn(async () => {})}
      reset={vi.fn()}
    />,
  );
}

describe('SettingsTab presenter controls and feedback', () => {
  it('shows placement failure beside the enabled taskbar control', () => {
    const html = render({ taskbarMode: true, taskbarError: 'Cannot verify taskbar bounds' });
    expect(html).toContain('role="alert"');
    expect(html).toContain('Cannot verify taskbar bounds');
  });

  it('does not show stale native errors while taskbar mode is off', () => {
    expect(render({ taskbarMode: false, taskbarError: 'Cannot verify taskbar bounds' })).not.toContain(
      'Cannot verify taskbar bounds',
    );
  });

  it('allows enabling overlay and taskbar independently', () => {
    const bothOn = render({ overlayVisible: true, taskbarMode: true, taskbarError: null });
    expect(bothOn).toContain('aria-label="Thanh nổi màn hình" checked=""');
    expect(bothOn).toContain('aria-label="Hiển thị trên Taskbar" checked=""');
  });

  it('maps taskbar phases to accurate Vietnamese labels', () => {
    expect(taskbarPhaseLabel('attached', true)).toBe('Đã gắn');
    expect(taskbarPhaseLabel('starting', true)).toBe('Đang khởi tạo');
    expect(taskbarPhaseLabel('degraded', true)).toBe('Bị giới hạn');
    expect(taskbarPhaseLabel('unsupported', true)).toBe('Không hỗ trợ');
    expect(taskbarPhaseLabel('disabled', false)).toBe('Đang tắt');
  });
});
