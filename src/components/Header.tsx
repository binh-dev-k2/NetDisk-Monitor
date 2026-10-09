import { Activity, Wifi } from 'lucide-react';

type HeaderProps = {
  tab: 'overview' | 'processes';
  setTab: (tab: 'overview' | 'processes') => void;
  interfaceCount: number;
  stale: boolean;
  loading: boolean;
};

export function Header({ tab, setTab, interfaceCount, stale, loading }: HeaderProps) {
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
          <i className={stale ? 'offline' : 'online'} /> {loading ? 'Đang kết nối' : stale ? 'Dữ liệu cũ' : 'Đang giám sát'}
        </span>
        <span>
          <Wifi size={14} /> Mạng: {interfaceCount} giao diện
        </span>
      </div>
    </header>
  );
}
