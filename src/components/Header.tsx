import { Activity, Wifi } from 'lucide-react';

type HeaderProps = {
  tab: 'overview' | 'processes';
  setTab: (tab: 'overview' | 'processes') => void;
  interfaceCount: number;
};

export function Header({ tab, setTab, interfaceCount }: HeaderProps) {
  return (
    <header className="content-header">
      <div className="header-left">
        <div className="app-brand">
          <Activity size={20} />
          <span>NetDisk Monitor</span>
        </div>
        <nav className="tabs" aria-label="Monitor views">
          <button
            className={tab === 'overview' ? 'active' : ''}
            onClick={() => setTab('overview')}
          >
            Tổng quan
          </button>
          <button
            className={tab === 'processes' ? 'active' : ''}
            onClick={() => setTab('processes')}
          >
            Theo Process
          </button>
        </nav>
      </div>
      <div className="system-status">
        <span>
          <i className="online" /> Đang giám sát
        </span>
        <span>
          <Wifi size={14} /> Mạng: {interfaceCount ? 'Đã kết nối' : 'Đang dò'}
        </span>
      </div>
    </header>
  );
}
