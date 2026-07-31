import { useState } from 'react';
import { Header } from './components/Header';
import { OverviewTab } from './components/OverviewTab';
import { ProcessTab } from './components/ProcessTab';
import { useMonitorSnapshot } from './hooks/useMonitorSnapshot';
import { useOverlaySettings } from './hooks/useOverlaySettings';
import { useProcessTable } from './hooks/useProcessTable';

export default function App() {
  const [tab, setTab] = useState<'overview' | 'processes'>('overview');

  const { snapshot, history, sessionNetwork, resetSession } = useMonitorSnapshot();
  const {
    overlaySettings,
    taskbarMode,
    overlayVisible,
    autostart,
    isPendingToggle,
    updateOverlaySettings,
    handleDisplayModeChange,
    toggleAutostart,
  } = useOverlaySettings();

  const processTableProps = useProcessTable(snapshot.processes, sessionNetwork);

  return (
    <div className="dashboard-container">
      <main className="content-area">
        <Header tab={tab} setTab={setTab} interfaceCount={snapshot.interfaces.length} />

        {tab === 'overview' ? (
          <OverviewTab
            snapshot={snapshot}
            history={history}
            overlayVisible={overlayVisible}
            taskbarMode={taskbarMode}
            isPendingToggle={isPendingToggle}
            overlaySettings={overlaySettings}
            autostart={autostart}
            handleDisplayModeChange={handleDisplayModeChange}
            updateOverlaySettings={updateOverlaySettings}
            toggleAutostart={toggleAutostart}
            reset={resetSession}
          />
        ) : (
          <ProcessTab {...processTableProps} processCount={snapshot.processes.length} />
        )}
      </main>
    </div>
  );
}
