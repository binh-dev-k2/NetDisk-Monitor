import { useState } from 'react';
import { DiskTab } from './components/DiskTab';
import { Header } from './components/Header';
import { HistoryTab } from './components/HistoryTab';
import { OverviewTab } from './components/OverviewTab';
import { ProcessTab } from './components/ProcessTab';
import { SettingsTab } from './components/SettingsTab';
import { useMonitorSnapshot } from './hooks/useMonitorSnapshot';
import { useOverlaySettings } from './hooks/useOverlaySettings';
import type { AppTab } from './types';

export default function App() {
  const [tab, setTab] = useState<AppTab>('overview');

  const {
    snapshot, processes, processNetworkError, history, status, processStatus,
    resetSession, resetError, resetPending, retrySystem, retryProcesses,
  } = useMonitorSnapshot(tab === 'processes');
  const {
    overlaySettings, taskbarMode, overlayVisible, overlayPinned, theme, autostart,
    isPendingOverlay, isPendingTaskbar, isPendingAutostart, error: settingsError,
    updateOverlaySettings, toggleOverlayVisible, toggleOverlayPinned,
    resetOverlayPosition, toggleTaskbarMode, changeTheme, toggleAutostart,
  } = useOverlaySettings();

  const isProcessInfoNotice = (message?: string | null) =>
    Boolean(message && (message.includes('SeSystemProfilePrivilege') || message.startsWith('ETW gaps:')));
  const rawProcessNotice = tab === 'processes' ? processNetworkError : snapshot.processNetworkStatusError;
  const privilegeNotice = isProcessInfoNotice(processNetworkError)
    ? processNetworkError
    : isProcessInfoNotice(snapshot.processNetworkStatusError)
      ? snapshot.processNetworkStatusError ?? null
      : null;

  const diagnostics = [
    snapshot.storageError,
    snapshot.taskbarError,
    isProcessInfoNotice(rawProcessNotice) ? null : rawProcessNotice,
    tab !== 'overview' ? status.error : null,
    settingsError,
    resetError,
  ].filter(Boolean);

  return (
    <div className="dashboard-container">
      <main className="content-area">
        <Header tab={tab} setTab={setTab} interfaceCount={snapshot.interfaces.length} stale={status.stale} loading={status.loading} />
        {diagnostics.length > 0 && <div className="diagnostics" role="alert">{Array.from(new Set(diagnostics)).join(' | ')}</div>}

        {tab === 'overview' && (
          <OverviewTab
            snapshot={snapshot}
            activeProcessCount={snapshot.activeProcessCount}
            history={history}
            loading={status.loading}
            error={status.error}
            overlayVisible={overlayVisible}
            taskbarMode={taskbarMode}
            resetPending={resetPending}
            onRetry={retrySystem}
            onOpenDisk={() => setTab('disk')}
            onOpenSettings={() => setTab('settings')}
            reset={resetSession}
          />
        )}
        <ProcessTab
          processes={processes}
          hidden={tab !== 'processes'}
          loading={processStatus.loading}
          stale={processStatus.stale}
          error={tab === 'processes' ? processStatus.error : null}
          privilegeNotice={privilegeNotice}
          onRetry={retryProcesses}
        />
        {tab === 'disk' && <DiskTab volumes={snapshot.volumes ?? []} />}
        <HistoryTab active={tab === 'history'} diskAvailable={snapshot.diskAvailable} />
        {tab === 'settings' && (
          <SettingsTab
            active={tab === 'settings'}
            snapshot={snapshot}
            overlayVisible={overlayVisible}
            overlayPinned={overlayPinned}
            taskbarMode={taskbarMode}
            taskbarError={snapshot.taskbarError}
            isPendingOverlay={isPendingOverlay}
            isPendingTaskbar={isPendingTaskbar}
            overlaySettings={overlaySettings}
            theme={theme}
            autostart={autostart}
            isPendingAutostart={isPendingAutostart}
            resetPending={resetPending}
            toggleOverlayVisible={toggleOverlayVisible}
            toggleOverlayPinned={toggleOverlayPinned}
            resetOverlayPosition={resetOverlayPosition}
            toggleTaskbarMode={toggleTaskbarMode}
            updateOverlaySettings={updateOverlaySettings}
            changeTheme={changeTheme}
            toggleAutostart={toggleAutostart}
            reset={resetSession}
          />
        )}
      </main>
    </div>
  );
}
