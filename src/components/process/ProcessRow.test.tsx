import { describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import type { ComponentProps } from 'react';
import { ProcessRow } from './ProcessRow';
import { groupProcesses } from '../../lib/process-model';
import type { ProcessMetric } from '../../types';

const process: ProcessMetric = {
  pid: '4321', identity: '4321:10', startTime: 10, name: 'agent64.exe', active: true,
  memoryBytes: 10, diskReadBps: 0, diskWriteBps: 0, sessionDiskRead: 100, sessionDiskWrite: 200,
  uptimeSecs: 60, networkConnections: 0, networkAvailable: false, networkDownBps: null, networkUpBps: null,
};
function render(processes: ProcessMetric[]) {
  return renderToStaticMarkup(<ProcessRow group={groupProcesses(processes)[0]} gridStyle={{}}
    visibleColumns={{ processCount: true, netDown: true, netUp: true, totalIO: true, totalNet: true }}
    isExpanded={true} toggleGroup={vi.fn()} handleKillProcess={vi.fn()} pendingKill={null} />);
}
describe('process row states', () => {
  it('labels currently unavailable network measurements accurately', () => {
    expect(render([process])).toContain('N/A');
    expect(render([process])).toContain('Lưu lượng mạng theo process hiện không khả dụng');
    expect(render([process])).not.toContain('Chưa hỗ trợ');
    expect(render([process])).toContain('PID 4321');
  });
  it('labels retired processes and removes their close action', () => {
    const html = render([{ ...process, active: false }]);
    expect(html).toContain('Đã kết thúc');
    expect(html).not.toContain('kill-process-btn');
  });
  it('provides keyboard group expansion and closes only active children', () => {
    const html = render([{ ...process, active: false }, { ...process, identity: '4321:20', startTime: 20 }]);
    expect(html).toContain('aria-expanded="true"');
    expect(html.match(/kill-process-btn/g)).toHaveLength(1);
  });
  it('shows known inactive network totals even when current rates are unavailable', () => {
    const html = render([{ ...process, active: false, sessionNetworkDown: 1024, sessionNetworkUp: 2048 }]);
    expect(html).toContain('3.0 KB');
    expect(html).toContain('N/A');
  });
  it('keeps incomplete grouped totals as N/A', () => {
    const html = render([
      { ...process, sessionNetworkDown: 1024, sessionNetworkUp: 2048 },
      { ...process, identity: '4321:20', startTime: 20, active: false, sessionNetworkDown: null, sessionNetworkUp: 0 },
    ]);
    expect(html.match(/N\/A/g)).toHaveLength(8);
  });
  it.each(['sessionNetworkDown', 'sessionNetworkUp'] as const)('invalidates row memoization when only %s changes', key => {
    type Props = ComponentProps<typeof ProcessRow>;
    const compare = (ProcessRow as unknown as { compare: (left: Props, right: Props) => boolean }).compare;
    const props: Props = { group: groupProcesses([{ ...process, sessionNetworkDown: 100, sessionNetworkUp: 200 }])[0],
      gridStyle: {}, visibleColumns: { totalNet: true }, isExpanded: false,
      toggleGroup: vi.fn(), handleKillProcess: vi.fn(), pendingKill: null };
    expect(compare(props, { ...props })).toBe(true);
    expect(compare(props, { ...props, group: groupProcesses([{ ...process, sessionNetworkDown: 100, sessionNetworkUp: 200, [key]: 300 }])[0] })).toBe(false);
  });
});
