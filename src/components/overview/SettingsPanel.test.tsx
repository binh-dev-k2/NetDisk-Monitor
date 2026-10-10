import { describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { SettingsPanel } from './SettingsPanel';
import { defaultOverlaySettings } from '../../lib/overlay-settings';

function render(taskbarMode: boolean, taskbarError: string | null) {
  return renderToStaticMarkup(<SettingsPanel overlayVisible={false} taskbarMode={taskbarMode}
    taskbarError={taskbarError} isPendingToggle={false} overlaySettings={defaultOverlaySettings}
    autostart={false} isPendingAutostart={false} resetPending={false}
    handleDisplayModeChange={vi.fn()} updateOverlaySettings={vi.fn()} toggleAutostart={vi.fn()} reset={vi.fn()} />);
}

describe('taskbar placement feedback', () => {
  it('shows placement failure beside the selected taskbar mode', () => {
    const html = render(true, 'Cannot verify taskbar bounds');
    expect(html).toContain('role="alert"');
    expect(html).toContain('Cannot verify taskbar bounds');
  });
  it('does not show stale native errors while the widget is hidden', () => {
    expect(render(false, 'Cannot verify taskbar bounds')).not.toContain('Cannot verify taskbar bounds');
  });
});
