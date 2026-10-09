import { useMemo, useState } from 'react';
import { Header } from './components/Header';
import { OverviewTab } from './components/OverviewTab';
import { ProcessTab } from './components/ProcessTab';
import { useMonitorSnapshot } from './hooks/useMonitorSnapshot';
import { useOverlaySettings } from './hooks/useOverlaySettings';

export default function App() {
  const [tab, setTab] = useState<'overview' | 'processes'>('overview');

  const { snapshot, processes, processNetworkError, history, status, processStatus, resetSession, resetError, resetPending } = useMonitorSnapshot();
  const {
    overlaySettings,
    taskbarMode,
    overlayVisible,
    autostart,
    isPendingToggle,
    isPendingAutostart,
    error: settingsError,
    updateOverlaySettings,
    handleDisplayModeChange,
    toggleAutostart,
  } = useOverlaySettings();

  const activeProcessCount = useMemo(() => processes.filter(process => process.active).length, [processes]);
  const diagnostics = [snapshot.storageError, snapshot.taskbarError, processNetworkError, status.error, processStatus.error, settingsError, resetError].filter(Boolean);

  return (
    <div className="dashboard-container">
      <main className="content-area">
        <Header tab={tab} setTab={setTab} interfaceCount={snapshot.interfaces.length} stale={status.stale} loading={status.loading} />
        {diagnostics.length > 0 && <div className="diagnostics" role="alert">{Array.from(new Set(diagnostics)).join(' | ')}</div>}

        {tab === 'overview' ? (
          <OverviewTab
            snapshot={snapshot}
            activeProcessCount={activeProcessCount}
            history={history}
            overlayVisible={overlayVisible}
            taskbarMode={taskbarMode}
            isPendingToggle={isPendingToggle}
            overlaySettings={overlaySettings}
            autostart={autostart}
            isPendingAutostart={isPendingAutostart}
            resetPending={resetPending}
            handleDisplayModeChange={handleDisplayModeChange}
            updateOverlaySettings={updateOverlaySettings}
            toggleAutostart={toggleAutostart}
            reset={resetSession}
          />
        ) : null}
        <ProcessTab processes={processes} hidden={tab !== 'processes'} loading={processStatus.loading} stale={processStatus.stale} />
      </main>
    </div>
  );
}
