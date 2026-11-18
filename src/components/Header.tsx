import type { ReactElement } from 'react';
import { Activity, HardDrive, History, Layers, LayoutDashboard, SlidersHorizontal, Wifi } from 'lucide-react';
import type { AppTab } from '../types';

type HeaderProps = {
  tab: AppTab;
  setTab: (tab: AppTab) => void;
  interfaceCount: number;
  stale: boolean;
  loading: boolean;
};

const TABS: ReadonlyArray<{ id: AppTab; label: string; icon: ReactElement }> = [
  { id: 'overview', label: 'Tổng quan', icon: <LayoutDashboard size={14} aria-hidden="true" /> },
  { id: 'processes', label: 'Ứng dụng', icon: <Layers size={14} aria-hidden="true" /> },
  { id: 'disk', label: 'Không gian đĩa', icon: <HardDrive size={14} aria-hidden="true" /> },
  { id: 'history', label: 'Lịch sử', icon: <History size={14} aria-hidden="true" /> },
  { id: 'settings', label: 'Cài đặt', icon: <SlidersHorizontal size={14} aria-hidden="true" /> },
];

export function Header({ tab, setTab, interfaceCount, stale, loading }: HeaderProps) {
  return (
    <header className="content-header">
      <div className="header-left">
        <div className="app-brand">
          <span className="app-brand-mark" aria-hidden="true"><Activity size={16} /></span>
          <span>NetDisk Monitor</span>
        </div>
        <nav className="tabs" aria-label="Monitor views">
          {TABS.map(item => (
            <button
              key={item.id}
              type="button"
              className={tab === item.id ? 'active' : ''}
              aria-current={tab === item.id ? 'page' : undefined}
              onClick={() => setTab(item.id)}
            >
              {item.icon}
              <span>{item.label}</span>
            </button>
          ))}
        </nav>
      </div>
      <div className="system-status">
        <span>
          <i className={stale ? 'offline' : 'online'} /> {loading ? 'Đang kết nối' : stale ? 'Dữ liệu cũ' : 'Đang giám sát'}
        </span>
        <span>
          <Wifi size={14} aria-hidden="true" /> Mạng: {interfaceCount} giao diện
        </span>
      </div>
    </header>
  );
}
