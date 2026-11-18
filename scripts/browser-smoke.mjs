import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_PATH || 'playwright');
const root = path.resolve(import.meta.dirname, '..');
const assets = path.resolve(process.env.BROWSER_SMOKE_ASSETS || path.join(root, 'dist-dev'));
const output = path.resolve(process.env.BROWSER_SMOKE_OUTPUT || path.join(os.tmpdir(), 'netdisk-monitor-browser'));
await fs.mkdir(output, { recursive: true });
const browser = await chromium.launch({ headless: true,
  ...(process.env.PLAYWRIGHT_CHANNEL ? { channel: process.env.PLAYWRIGHT_CHANNEL } : {}) });
const page = await browser.newPage();
const errors = [];
page.on('pageerror', error => errors.push(error.message));
page.on('response', response => {
  if (response.url().startsWith('http://monitor.test/') && response.status() >= 400) errors.push(`Asset ${response.status()}: ${response.url()}`);
});
page.on('dialog', dialog => dialog.accept());
await page.route('**/*', async route => {
  const url = new URL(route.request().url());
  if (url.origin !== 'http://monitor.test') return route.abort();
  const file = path.resolve(assets, '.' + (url.pathname === '/' ? '/index.html' : decodeURIComponent(url.pathname)));
  if (!file.startsWith(assets + path.sep)) return route.fulfill({ status: 403, body: '' });
  const type = file.endsWith('.js') ? 'text/javascript' : file.endsWith('.css') ? 'text/css' : 'text/html';
  try { await route.fulfill({ body: await fs.readFile(file), contentType: type }); }
  catch { await route.fulfill({ status: 404, body: '' }); }
});
await page.addInitScript(() => {
  let id = 0;
  const callbacks = new Map();
  const listeners = new Map();
  const settings = { showNetworkDown: true, showNetworkUp: true, showDiskRead: true,
    showDiskWrite: true, showSessionTotal: true, showSessionDuration: true, showRam: true, opacity: 72 };
  const processes = Array.from({ length: 42 }, (_, index) => ({
    pid: String(100 + index), identity: `${100 + index}:1000:generation-${index}`, startTime: 1000,
    active: index !== 1, name: index < 2 ? 'network-regression.exe' : index < 4 ? 'partial-network.exe'
      : index === 4 ? 'long-process-name-' + 'very-long-'.repeat(20) + '.exe' : `worker-${index}.exe`,
    memoryBytes: 1024 * 1024, diskReadBps: 1024, diskWriteBps: 2048,
    sessionDiskRead: 1000000, sessionDiskWrite: 2000000, uptimeSecs: 90061,
    networkConnections: 2, networkAvailable: index < 2 && index !== 1,
    networkDownBps: index < 2 ? 1024 : null, networkUpBps: index < 2 ? 512 : null,
    sessionNetworkDown: index === 3 ? null : 1024, sessionNetworkUp: 1024,
  }));
  let snapshot = { sequence: 1, processSequence: 1, diskAvailable: true, networkDownBps: 12000, networkUpBps: 4000,
    diskReadBps: 1000000, diskWriteBps: 10000, sessionNetworkDown: 1200000,
    sessionNetworkUp: 400000, sessionDiskRead: 1000000, sessionDiskWrite: 10000,
    sessionDurationSecs: 90061, interfaces: ['Ethernet'], totalMemoryBytes: 16000000000,
    usedMemoryBytes: 4000000000, storageError: null, taskbarError: null,
    taskbarStatus: { phase: 'disabled', desired: false, attached: false, reason: null },
    processNetworkError: null, processes,
    cpuUsagePercent: 18,
    volumes: [{ mountPoint: 'C:\\', name: 'OS', fileSystem: 'NTFS', totalBytes: 500000000000, availableBytes: 150000000000, usedBytes: 350000000000, isRemovable: false }],
    activeProcessCount: null, processNetworkStatusError: null };
  const processSnapshot = () => ({ processSequence: snapshot.processSequence,
    processes: snapshot.processes, processNetworkError: snapshot.processNetworkError });
  const systemSnapshot = () => {
    const { processes, processSequence, processNetworkError, ...system } = snapshot;
    return system;
  };
  let overlayVisible = true;
  let taskbarMode = false;
  let pinned = true;
  let theme = 'system';
  let processViewActive = false;
  let storageOverview = {
    appDataDir: 'C:\\Users\\Test\\AppData\\Roaming\\com.binhc.netdiskmonitor',
    webViewDataDir: 'C:\\Users\\Test\\AppData\\Local\\com.binhc.netdiskmonitor',
    totalAppManagedBytes: 1360000,
    maxAppManagedBytes: 1876032,
    totalWebViewBytes: 8388608,
    items: [
      {
        id: 'history',
        label: 'Lịch sử lưu lượng (vòng đệm)',
        path: 'C:\\Users\\Test\\AppData\\Roaming\\com.binhc.netdiskmonitor\\history-v1.bin',
        bytes: 1347584,
        maxBytes: 1347584,
        recordCount: 30,
        clearable: true,
        detail: '20 bản ghi phút (tối đa 10080), 10 bản ghi ngày (tối đa 366) · Cố định kích thước',
      },
      {
        id: 'diagnostics',
        label: 'Nhật ký chẩn đoán (xoay vòng)',
        path: 'C:\\Users\\Test\\AppData\\Roaming\\com.binhc.netdiskmonitor\\diagnostics',
        bytes: 12392,
        maxBytes: 528384,
        recordCount: 2,
        clearable: true,
        detail: 'Gồm events.jsonl, events.1.jsonl và last-fatal.txt · Tự xoay vòng tối đa 516 KB',
      },
      {
        id: 'preferences',
        label: 'Cấu hình người dùng & khóa phiên',
        path: 'C:\\Users\\Test\\AppData\\Roaming\\com.binhc.netdiskmonitor\\preferences-v2.bin',
        bytes: 24,
        maxBytes: 64,
        recordCount: 2,
        clearable: false,
        detail: 'Lưu vị trí thanh nổi, giao diện, taskbar (được giữ khi dọn)',
      },
      {
        id: 'webview',
        label: 'Bộ nhớ đệm giao diện WebView2',
        path: 'C:\\Users\\Test\\AppData\\Local\\com.binhc.netdiskmonitor',
        bytes: 8388608,
        maxBytes: null,
        recordCount: 42,
        clearable: false,
        detail: 'Cache trình duyệt nội bộ do hệ điều hành quản lý khi UI đang mở',
      },
    ],
    memory: { iconCacheCount: 12, iconCacheLimit: 512, terminatedProcessCount: 4, terminatedProcessLimit: 512 },
  };
  let cleanupReport = {
    totalReclaimableBytes: 157286400,
    totalFiles: 52,
    rules: [
      { id: 'temp_files', category: 'Hệ thống', label: 'Tệp tạm hệ thống (> 24 giờ)', description: 'Tệp tạm cũ', path: 'C:\\Users\\Test\\AppData\\Local\\Temp', bytes: 52428800, fileCount: 12, exists: true, safetyNote: 'Giữ tệp < 24h' },
      { id: 'npm_cache', category: 'Developer', label: 'npm Package Cache', description: 'Gói npm cache', path: 'C:\\Users\\Test\\AppData\\Local\\npm-cache', bytes: 104857600, fileCount: 40, exists: true, safetyNote: 'Tự tải lại' },
    ],
  };
  const emit = (event, payload) => {
    for (const [eventId, listener] of listeners) {
      if (listener.event === event) callbacks.get(listener.handler)?.({ event, id: eventId, payload });
    }
  };
  window.__testCalls = [];
  window.__testEmit = emit;
  window.__testListeners = event => [...listeners.values()].filter(listener => listener.event === event).length;
  window.__testProcessViewActive = () => processViewActive;
  window.__testSystemUpdate = change => {
    snapshot = { ...snapshot, ...change, sequence: snapshot.sequence + 1 };
    emit('metrics://snapshot', systemSnapshot());
  };
  window.__testProcessFetchError = null;
  window.__testProcesses = () => snapshot.processes;
  window.__testProcessUpdate = change => {
    snapshot = { ...snapshot, ...change, processSequence: snapshot.processSequence + 1 };
    emit('processes://snapshot', processSnapshot());
  };
  window.__TAURI_INTERNALS__ = {
    transformCallback(callback) { callbacks.set(++id, callback); return id; },
    async invoke(command, args = {}) {
      window.__testCalls.push({ command, args });
      if (command === 'plugin:event|listen') { listeners.set(++id, args); return id; }
      if (command === 'plugin:event|unlisten') { listeners.delete(args.eventId); return; }
      if (command === 'get_snapshot') return snapshot;
      if (command === 'get_system_snapshot') return systemSnapshot();
      if (command === 'get_process_snapshot') {
        if (window.__testProcessFetchError) throw new Error(window.__testProcessFetchError);
        return processSnapshot();
      }
      if (command === 'set_process_view_active') { processViewActive = Boolean(args.active); return null; }
      if (command === 'get_runtime_health') return {
        processId: 4242,
        ignoredExitRequests: 1,
        workers: [
          { kind: 'system', status: 'running', heartbeatAgeMillis: 45, ticks: 120, lastError: null },
          { kind: 'processes', status: 'running', heartbeatAgeMillis: 120, ticks: 24, lastError: null },
          { kind: 'storage', status: 'running', heartbeatAgeMillis: 80, ticks: 120, lastError: null },
        ],
        logging: { droppedEvents: 0, lastError: null, active: true },
      };
      if (command === 'get_theme') return theme;
      if (command === 'set_theme') { theme = args.theme; emit('app://theme', theme); return theme; }
      if (command === 'reset_overlay_position') return null;
      if (command === 'get_overlay_settings') return { ...settings };
      if (command === 'update_overlay_settings') {
        Object.assign(settings, args.settings); emit('overlay://settings', { ...settings }); return { ...settings };
      }
      if (command === 'get_overlay_visible') return overlayVisible;
      if (command === 'get_overlay_pinned') return pinned;
      if (command === 'get_taskbar_mode') return taskbarMode;
      if (command === 'get_autostart') return false;
      if (command === 'set_taskbar_mode') {
        taskbarMode = args.enabled;
        snapshot = {
          ...snapshot,
          sequence: snapshot.sequence + 1,
          taskbarStatus: { phase: taskbarMode ? 'attached' : 'disabled', desired: taskbarMode, attached: taskbarMode, reason: null },
        };
        emit('taskbar://mode', taskbarMode);
        emit('metrics://snapshot', systemSnapshot());
        return taskbarMode;
      }
      if (command === 'set_overlay_visible') { overlayVisible = args.visible; emit('overlay://visibility', overlayVisible); return overlayVisible; }
      if (command === 'set_overlay_pinned') { pinned = args.pinned; emit('overlay://pinned', pinned); return pinned; }
      if (command === 'set_autostart') return args.enabled;
      if (command === 'export_csv_file') return 'C:/Users/Test/Downloads/' + args.filename;
      if (command === 'scan_storage_info') return structuredClone(storageOverview);
      if (command === 'open_storage_folder') {
        return args.target === 'webview' ? storageOverview.webViewDataDir : storageOverview.appDataDir;
      }
      if (command === 'clear_storage_data') {
        const t = args.target;
        if (t === 'caches' || t === 'all_clearable') {
          storageOverview.memory = { ...storageOverview.memory, iconCacheCount: 0, terminatedProcessCount: 0 };
        }
        if (t === 'history' || t === 'all_clearable') {
          storageOverview.items[0] = { ...storageOverview.items[0], recordCount: 0 };
          emit('history://updated', null);
        }
        if (t === 'diagnostics' || t === 'all_clearable') {
          storageOverview.items[1] = { ...storageOverview.items[1], bytes: 0, recordCount: 0 };
          storageOverview.totalAppManagedBytes = 1347584 + 24;
        }
        return structuredClone(storageOverview);
      }
      if (command === 'get_process_icons') return Object.fromEntries((args.items || []).map(item => [
        item.name,
        item.name === 'network-regression.exe'
          ? 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=='
          : null,
      ]));
      if (command === 'get_history') return Array.from({ length: 30 }, (_, index) => ({
        timestampSecs: 1700000000 - index * (args.resolution === 'day' ? 86400 : 60), durationMillis: 60000, bytes: [600000, 100000, 400000, 200000],
      }));
      if (command === 'inspect_process_path') return 'C:\\Program Files\\Test\\' + (args.pid === '100' ? 'network-regression.exe' : 'app.exe');
      if (command === 'scan_cleanup_rules') return structuredClone(cleanupReport);
      if (command === 'execute_cleanup_rules') {
        const ids = new Set(args.rules || []);
        let freedBytes = 0;
        let deletedFiles = 0;
        cleanupReport = {
          ...cleanupReport,
          rules: cleanupReport.rules.map(rule => {
            if (!ids.has(rule.id)) return rule;
            freedBytes += rule.bytes;
            deletedFiles += rule.fileCount;
            return { ...rule, bytes: 0, fileCount: 0 };
          }),
        };
        cleanupReport.totalReclaimableBytes = cleanupReport.rules.reduce((sum, rule) => sum + rule.bytes, 0);
        cleanupReport.totalFiles = cleanupReport.rules.reduce((sum, rule) => sum + rule.fileCount, 0);
        return { freedBytes, deletedFiles, skippedLockedFiles: 1, refreshed: structuredClone(cleanupReport) };
      }
      if (command === 'analyze_disk_space') {
        if (args.target === 'C:\\Users\\Test\\Downloads\\Installers') {
          return {
            scannedPath: 'C:\\Users\\Test\\Downloads\\Installers',
            parentPath: 'C:\\Users\\Test\\Downloads',
            totalBytes: 629145600,
            scannedFiles: 4,
            scannedDirs: 1,
            truncated: false,
            children: [{ name: 'win11-sdk.iso', path: 'C:\\Users\\Test\\Downloads\\Installers\\win11-sdk.iso', bytes: 629145600, isDir: false }],
            largestFiles: [{ name: 'win11-sdk.iso', path: 'C:\\Users\\Test\\Downloads\\Installers\\win11-sdk.iso', bytes: 629145600 }],
            devArtifacts: [],
          };
        }
        return {
          scannedPath: 'C:\\Users\\Test\\Downloads',
          parentPath: 'C:\\Users\\Test',
          totalBytes: 1048576000,
          scannedFiles: 42,
          scannedDirs: 7,
          truncated: false,
          children: [
            { name: 'Installers', path: 'C:\\Users\\Test\\Downloads\\Installers', bytes: 629145600, isDir: true },
            { name: 'Projects', path: 'C:\\Users\\Test\\Downloads\\Projects', bytes: 314572800, isDir: true },
            { name: 'archive.zip', path: 'C:\\Users\\Test\\Downloads\\archive.zip', bytes: 104857600, isDir: false },
          ],
          largestFiles: [{ name: 'archive.zip', path: 'C:\\Users\\Test\\Downloads\\archive.zip', bytes: 104857600 }],
          devArtifacts: [{ name: 'node_modules', path: 'C:\\Users\\Test\\Downloads\\Projects\\demo\\node_modules', bytes: 314572800 }],
        };
      }
      if (command === 'reveal_disk_path') return args.path;
      return null;
    },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener(_event, listener) { listeners.delete(listener); } };
});

async function assertLayout(label) {
  const problems = await page.evaluate(() => {
    const result = [];
    if (document.documentElement.scrollWidth > innerWidth) result.push('document horizontal overflow');
    for (const element of document.querySelectorAll('button, input, select, .column-menu-dropdown, .pagination')) {
      if (!element.getClientRects().length || element.closest('[hidden]') || element.closest('.process-table, .history-scroll')) continue;
      const rect = element.getBoundingClientRect();
      if (rect.left < -1 || rect.right > innerWidth + 1 || rect.width <= 0 || rect.height <= 0) {
        result.push(`${element.getAttribute('aria-label') || element.textContent || element.className}: ${rect.left},${rect.right}`);
      }
    }
    return result;
  });
  assert.deepEqual(problems, [], `Control bounds at ${label}`);
}

const commandCount = command => page.evaluate(command => window.__testCalls.filter(call => call.command === command).length, command);
async function assertSystemOnly(label) {
  assert(await commandCount('get_system_snapshot') > 0, `${label}: missing narrow system bootstrap`);
  assert.equal(await commandCount('get_snapshot'), 0, `${label}: legacy bootstrap`);
  assert.equal(await commandCount('get_process_snapshot'), 0, `${label}: process bootstrap`);
  assert.equal(await commandCount('get_process_icons'), 0, `${label}: icons requested outside Applications`);
  assert.equal(await commandCount('scan_storage_info'), 0, `${label}: storage scanned outside Settings`);
  assert.equal(await commandCount('scan_cleanup_rules'), 0, `${label}: cleanup scanned outside Disk tab`);
  assert.equal(await commandCount('analyze_disk_space'), 0, `${label}: disk space analyzed outside Disk tab`);
  assert.equal(await page.evaluate(() => window.__testListeners('processes://snapshot')), 0, `${label}: process listener`);
  assert.equal(await page.evaluate(() => window.__testCalls.filter(call => call.command === 'plugin:event|listen' && call.args.event === 'processes://snapshot').length), 0,
    `${label}: process subscription attempted`);
}

try {
  await page.clock.install();
  for (const width of [1366, 780, 420]) {
    await page.setViewportSize({ width, height: 800 });
    await page.goto('http://monitor.test/');
    await page.locator('.metric-grid').waitFor();
    await assertSystemOnly('initial Overview');
    assert.equal(await commandCount('get_history'), 0, 'Initial Overview must not fetch history');
    await page.getByText('CPU 18%', { exact: true }).waitFor();
    await page.locator('.overview-volumes').getByText('C:\\', { exact: true }).waitFor();
    assert.equal(await page.evaluate(() => window.__testProcessViewActive()), false);
    const processSummary = page.locator('.session-cell').filter({ hasText: 'Tiến trình:' }).locator('b');
    assert.equal(await processSummary.textContent(), 'N/A', 'Unknown process count became zero');
    await page.evaluate(() => window.__testSystemUpdate({ activeProcessCount: 0 }));
    await page.getByText('0 đang chạy', { exact: true }).waitFor();
    await page.evaluate(() => window.__testSystemUpdate({ activeProcessCount: 73 }));
    await page.getByText('73 đang chạy', { exact: true }).waitFor();
    await assertLayout(`${width} overview`);
    await page.screenshot({ path: path.join(output, `overview-${width}.png`), fullPage: true });

    await page.getByRole('button', { name: 'Ứng dụng', exact: true }).click();
    const row = page.locator('.process-row.parent').filter({ has: page.getByText('network-regression.exe', { exact: true }) });
    await row.waitFor();
    await row.locator('img.process-icon-img').waitFor();
    assert.equal(await page.evaluate(() => window.__testProcessViewActive()), true);
    assert.equal(await commandCount('get_process_snapshot'), 1, 'Process entry must fetch exactly once');
    assert.equal(await page.evaluate(() => window.__testListeners('processes://snapshot')), 1, 'Process entry must listen');
    assert.equal(await page.evaluate(() => window.__testCalls.filter(call => call.command === 'plugin:event|listen' && call.args.event === 'processes://snapshot').length), 1,
      'Process entry must subscribe exactly once');
    assert.equal(await row.locator(':scope > span').last().textContent(), '4.0 KB', 'Inactive child totals were lost');
    const partial = page.locator('.process-row.parent').filter({ has: page.getByText('partial-network.exe', { exact: true }) });
    assert.equal(await partial.locator(':scope > span').last().textContent(), 'N/A', 'Unknown grouped total became a partial sum');
    assert.equal(await partial.locator('.down').getAttribute('title'), 'Lưu lượng mạng theo process hiện không khả dụng');

    await page.evaluate(() => window.__testProcessUpdate({
      processes: window.__testProcesses().map(process => process.pid === '100' ? { ...process, sessionNetworkDown: 4096 } : process),
    }));
    await page.waitForFunction(() => [...document.querySelectorAll('.process-row.parent')].some(element =>
      element.querySelector('b')?.textContent === 'network-regression.exe' && element.lastElementChild.textContent === '7.0 KB'));
    await page.evaluate(() => window.__testProcessUpdate({
      processes: window.__testProcesses().map(process => process.pid === '100' ? { ...process, sessionNetworkUp: null } : process),
    }));
    await page.waitForFunction(() => [...document.querySelectorAll('.process-row.parent')].some(element =>
      element.querySelector('b')?.textContent === 'network-regression.exe' && element.lastElementChild.textContent === 'N/A'));
    await page.evaluate(() => window.__testProcessUpdate({
      processes: window.__testProcesses().map(process => process.pid === '100' ? { ...process, sessionNetworkUp: 1024 } : process),
    }));
    await page.waitForFunction(() => [...document.querySelectorAll('.process-row.parent')].some(element =>
      element.querySelector('b')?.textContent === 'network-regression.exe' && element.lastElementChild.textContent === '7.0 KB'));

    const diagnostic = 'Đo lưu lượng process hiện không khả dụng (ETW: quyền truy cập)';
    await page.evaluate(message => window.__testProcessUpdate({ processNetworkError: message }), diagnostic);
    await page.getByRole('alert').filter({ hasText: diagnostic }).waitFor();
    await assertLayout(`${width} diagnostic`);
    await page.evaluate(() => window.__testProcessUpdate({ processNetworkError: null }));
    await page.waitForFunction(() => !document.querySelector('.content-area > .diagnostics'));

    await row.getByRole('button', { name: 'Mở nhóm network-regression.exe' }).click();
    const children = row.locator('..').locator('.process-row.child');
    assert.equal(await children.count(), 2);
    assert.equal(await children.locator('.reveal-process-btn').count(), 1, 'Active child should expose inspect location button');
    await children.getByRole('button', { name: 'Mở vị trí network-regression.exe (PID 100)' }).click();
    await page.getByRole('status').filter({ hasText: 'Vị trí tệp:' }).waitFor();
    assert.deepEqual(await page.evaluate(() => window.__testCalls.find(call => call.command === 'inspect_process_path').args),
      { pid: '100', startTime: 1000, identity: '100:1000:generation-0', reveal: true }, 'Inspect location did not pass exact identity');
    assert.equal(await children.locator('.kill-process-btn').count(), 1, 'Retired child has a kill control');
    await children.getByRole('button', { name: 'Đóng network-regression.exe (PID 100)' }).click();
    assert.deepEqual(await page.evaluate(() => window.__testCalls.find(call => call.command === 'kill_process').args),
      { pid: '100', startTime: 1000, identity: '100:1000:generation-0' }, 'Kill did not pass the exact backend identity');

    await page.getByRole('textbox', { name: 'Tìm process theo tên hoặc PID' }).fill('network-regression');
    await page.getByRole('button', { name: 'Xuất CSV', exact: true }).click();
    await page.getByRole('status').filter({ hasText: 'Đã lưu CSV:' }).waitFor();
    const csv = await page.evaluate(() => window.__testCalls.find(call => call.command === 'export_csv_file').args.content);
    assert(csv.includes('Mạng phiên đã đo (Bytes)') && csv.includes('"100 | 101"') && csv.split('\r\n')[1].endsWith(',7168'), 'Grouped process CSV totals are incorrect');
    await page.getByRole('button', { name: 'Xóa tìm kiếm' }).click();

    await page.getByRole('button', { name: 'Cột hiển thị', exact: true }).click();
    await assertLayout(`${width} column menu`);
    await page.getByRole('checkbox', { name: 'Mạng phiên', exact: true }).uncheck();
    assert.equal(await page.getByRole('button', { name: 'Mạng phiên: sắp xếp', exact: true }).count(), 0);
    await page.getByRole('checkbox', { name: 'Mạng phiên', exact: true }).check();
    await page.keyboard.press('Escape');
    await page.getByRole('button', { name: 'Mạng phiên: sắp xếp', exact: true }).click();
    await page.getByRole('combobox', { name: 'Số nhóm mỗi trang' }).selectOption('10');
    await page.getByRole('button', { name: 'Sau', exact: true }).click();
    await page.locator('.page-indicator').filter({ hasText: 'Trang 2 /' }).waitFor();
    await page.getByRole('button', { name: 'Trang đầu', exact: true }).click();
    await assertLayout(`${width} processes`);
    const scroll = await page.locator('.process-table').evaluate(element => {
      element.scrollLeft = element.scrollWidth;
      const result = { width: element.clientWidth, total: element.scrollWidth, offset: element.scrollLeft };
      element.scrollLeft = 0;
      return result;
    });
    if (width < 1366) assert(scroll.total > scroll.width && scroll.offset > 0, 'Process table cannot scroll horizontally');
    await page.screenshot({ path: path.join(output, `processes-${width}.png`), fullPage: true });

    await page.getByRole('textbox', { name: 'Tìm process theo tên hoặc PID' }).fill('network-regression');
    await page.getByRole('button', { name: 'Tổng quan', exact: true }).click();
    await page.locator('.metric-grid').waitFor();
    await page.waitForFunction(() => window.__testListeners('processes://snapshot') === 0);
    assert.equal(await page.evaluate(() => window.__testProcessViewActive()), false);
    const hiddenFetches = await commandCount('get_process_snapshot');
    const hiddenSubscriptions = await page.evaluate(() => window.__testCalls.filter(call => call.command === 'plugin:event|listen' && call.args.event === 'processes://snapshot').length);
    const hiddenTable = await page.locator('.processes').innerHTML();
    const summaryDiagnostic = 'Process summary unavailable';
    await page.evaluate(message => {
      window.__testProcessUpdate({ processes: window.__testProcesses().map(process =>
        process.pid === '100' ? { ...process, sessionNetworkDown: 8192 } : process) });
      window.__testSystemUpdate({ activeProcessCount: null, processNetworkStatusError: message });
    }, summaryDiagnostic);
    await page.getByRole('alert').filter({ hasText: summaryDiagnostic }).waitFor();
    assert.equal(await processSummary.textContent(), 'N/A', 'Unavailable summary count became zero');
    await page.clock.fastForward(20000);
    assert.equal(await commandCount('get_process_snapshot'), hiddenFetches, 'Hidden Process polled');
    assert.equal(await page.evaluate(() => window.__testListeners('processes://snapshot')), 0, 'Hidden Process resubscribed');
    assert.equal(await page.evaluate(() => window.__testCalls.filter(call => call.command === 'plugin:event|listen' && call.args.event === 'processes://snapshot').length),
      hiddenSubscriptions, 'Hidden Process attempted a subscription');
    assert.equal(await page.locator('.processes').innerHTML(), hiddenTable, 'Hidden table updated');
    await page.getByRole('button', { name: 'Ứng dụng', exact: true }).click();
    await page.waitForFunction(() => [...document.querySelectorAll('.process-row.parent')].some(element =>
      element.querySelector('b')?.textContent === 'network-regression.exe' && element.lastElementChild.textContent === '11.0 KB'));
    assert.equal(await commandCount('get_process_snapshot'), hiddenFetches + 1, 'Reentry must fetch latest rows once');
    assert.equal(await page.evaluate(() => window.__testListeners('processes://snapshot')), 1);
    assert.equal(await page.getByRole('textbox', { name: 'Tìm process theo tên hoặc PID' }).inputValue(), 'network-regression', 'Search state lost');
    assert.equal(await page.getByRole('combobox', { name: 'Số nhóm mỗi trang' }).inputValue(), '10', 'Page size lost');
    assert.equal(await row.locator('..').locator('.process-row.child').count(), 2, 'Expansion state lost');
    await page.getByRole('button', { name: 'Tổng quan', exact: true }).click();
    await page.waitForFunction(() => window.__testListeners('processes://snapshot') === 0);
    await page.evaluate(() => { window.__testProcessFetchError = 'Process IPC unavailable'; });
    await page.getByRole('button', { name: 'Ứng dụng', exact: true }).click();
    await page.getByRole('alert').filter({ hasText: 'Process IPC unavailable' }).waitFor();
    await page.getByRole('button', { name: 'Tổng quan', exact: true }).click();
    await page.getByRole('alert').filter({ hasText: summaryDiagnostic }).waitFor();
    assert.equal(await page.getByRole('alert').filter({ hasText: 'Process IPC unavailable' }).count(), 0, 'Hidden process IPC error leaked');
    await page.evaluate(() => {
      window.__testProcessFetchError = null;
      window.__testSystemUpdate({ activeProcessCount: 74, processNetworkStatusError: null });
    });
    await page.waitForFunction(() => !document.querySelector('.content-area > .diagnostics'));
    assert.equal(await commandCount('get_snapshot'), 0, 'Dashboard used legacy snapshot');

    await page.getByRole('button', { name: 'Lịch sử', exact: true }).click();
    await page.locator('.history-scroll tbody tr').first().waitFor();
    await assertLayout(`${width} history`);
    await page.getByRole('button', { name: 'Trang lịch sử sau', exact: true }).click();
    await page.getByRole('combobox', { name: 'Độ chi tiết lịch sử' }).selectOption('minute');
    await page.locator('.history-scroll tbody tr').first().waitFor();
    await page.getByRole('button', { name: 'Làm mới lịch sử', exact: true }).click();
    await page.locator('.history-scroll tbody tr').first().waitFor();
    await page.getByRole('button', { name: 'Xuất lịch sử CSV', exact: true }).click();
    await page.getByRole('status').filter({ hasText: 'Đã lưu CSV:' }).waitFor();
    assert(await page.evaluate(() => window.__testCalls.some(call => call.command === 'export_csv_file' && call.args.content.includes('NetworkDownBytes'))), 'History CSV is not exported');
    await page.screenshot({ path: path.join(output, `history-${width}.png`), fullPage: true });

    await page.getByRole('button', { name: 'Không gian đĩa', exact: true }).click();
    await page.locator('.space-analyzer-panel .disk-path-bar').getByText('C:\\Users\\Test\\Downloads', { exact: true }).waitFor();
    await page.locator('.smart-cleaner-panel').getByText('Tệp tạm hệ thống (> 24 giờ)').waitFor();
    await assertLayout(`${width} disk`);
    await page.getByRole('button', { name: 'Thư mục Build / Dev (1)', exact: true }).click();
    await page.locator('.space-analyzer-panel').getByText('node_modules', { exact: true }).waitFor();
    await page.getByRole('button', { name: 'Mở thư mục Installers: 600.0 MB', exact: true }).click();
    await page.locator('.space-analyzer-panel .disk-path-bar').getByText('C:\\Users\\Test\\Downloads\\Installers', { exact: true }).waitFor();
    await page.getByRole('button', { name: 'Lên thư mục cha', exact: true }).click();
    await page.locator('.space-analyzer-panel .disk-path-bar').getByText('C:\\Users\\Test\\Downloads', { exact: true }).waitFor();
    await page.getByRole('button', { name: 'Dọn mục đã chọn' }).click();
    await page.getByRole('button', { name: 'Xác nhận dọn ngay', exact: true }).click();
    await page.locator('.smart-cleaner-panel').getByRole('status').filter({ hasText: 'Đã giải phóng' }).waitFor();
    assert.deepEqual(await page.evaluate(() => window.__testCalls.find(call => call.command === 'execute_cleanup_rules').args),
      { rules: ['temp_files'] }, 'Cleaner did not pass expected selected rules');
    await assertLayout(`${width} disk cleaned`);
    await page.screenshot({ path: path.join(output, `disk-${width}.png`), fullPage: true });

    await page.getByRole('button', { name: 'Cài đặt', exact: true }).click();
    await page.locator('.diagnostics-panel').getByText('Thu thập hệ thống (1s)').waitFor();
    await page.locator('.storage-panel').getByText('C:\\Users\\Test\\AppData\\Roaming\\com.binhc.netdiskmonitor').waitFor();
    await page.getByRole('button', { name: 'Mở thư mục dữ liệu', exact: true }).click();
    await page.locator('.storage-panel').getByRole('status').filter({ hasText: 'Đã mở thư mục:' }).waitFor();
    await page.getByRole('button', { name: 'Xóa nhật ký', exact: true }).click();
    await page.locator('.storage-panel').getByRole('status').filter({ hasText: 'Đã dọn sạch nhật ký chẩn đoán.' }).waitFor();
    await page.getByRole('button', { name: 'Xóa lịch sử', exact: true }).click();
    await page.getByRole('button', { name: 'Xác nhận xóa lịch sử', exact: true }).click();
    await page.locator('.storage-panel').getByRole('status').filter({ hasText: 'Đã xóa toàn bộ lịch sử lưu lượng' }).waitFor();
    assert(await page.evaluate(() => window.__testCalls.some(call => call.command === 'clear_storage_data' && call.args.target === 'history')));
    await page.getByRole('checkbox', { name: 'Khởi động cùng hệ thống' }).check();
    await page.waitForFunction(() => document.querySelector('.autostart-toggle input').checked);
    await page.getByRole('checkbox', { name: 'Hiển thị trên Taskbar', exact: true }).check();
    assert.equal(await page.getByRole('checkbox', { name: 'Thanh nổi màn hình', exact: true }).isChecked(), true, 'Enabling taskbar hid overlay');
    await page.getByRole('slider', { name: 'Độ mờ thanh nổi' }).fill('60');
    await page.getByRole('combobox', { name: 'Giao diện' }).selectOption('light');
    await page.waitForFunction(() => document.documentElement.dataset.theme === 'light');
    await assertLayout(`${width} settings light`);
    await page.getByRole('combobox', { name: 'Giao diện' }).selectOption('dark');
    await page.waitForFunction(() => document.documentElement.dataset.theme === 'dark');
    await page.getByRole('button', { name: 'Đặt lại vị trí thanh nổi', exact: true }).click();
    await page.getByRole('checkbox', { name: 'Tốc độ tải xuống', exact: true }).uncheck();
    await page.getByRole('checkbox', { name: 'Tốc độ tải xuống', exact: true }).check();
    await page.getByRole('button', { name: 'Reset Phiên', exact: true }).click();
    assert(await page.evaluate(() => window.__testCalls.some(call => call.command === 'reset_session_totals')));
    await assertLayout(`${width} settings`);
    await page.screenshot({ path: path.join(output, `settings-${width}.png`) });
    console.log(`viewport ${width}: 5-view nav, scoped streams, disk analyzer/cleaner, independent presenters, theme, diagnostics, CSV and layout passed`);
  }

  await page.setViewportSize({ width: 336, height: 164 });
  await page.goto('http://monitor.test/meter.html');
  await page.locator('.meter-item').first().waitFor();
  await assertSystemOnly('overlay');
  await assertLayout('overlay 336x164');
  await page.evaluate(() => window.__testEmit('taskbar://mode', true));
  await page.locator('.meter-item').first().waitFor();
  await page.locator('.meter-item').first().dispatchEvent('pointerdown', { button: 0, pointerType: 'mouse' });
  assert(await page.evaluate(() => window.__testCalls.some(call => call.command === 'begin_overlay_drag')), 'Overlay does not request drag');
  const drags = await page.evaluate(() => window.__testCalls.filter(call => call.command === 'begin_overlay_drag').length);
  await page.getByRole('button', { name: 'Bỏ ghim trên cùng', exact: true }).click();
  await page.getByRole('button', { name: 'Ghim trên cùng', exact: true }).waitFor();
  assert.equal(await page.evaluate(() => window.__testCalls.filter(call => call.command === 'begin_overlay_drag').length), drags, 'Pin control began a drag');
  await page.evaluate(() => window.__testEmit('metrics://snapshot', { sequence: 2, diskAvailable: false,
    networkDownBps: 1e15, networkUpBps: 1e15, diskReadBps: 0, diskWriteBps: 0,
    sessionNetworkDown: 1e15, sessionNetworkUp: 1e15, sessionDiskRead: 0, sessionDiskWrite: 0,
    sessionDurationSecs: 90061, interfaces: [], totalMemoryBytes: 1e15, usedMemoryBytes: 1e15,
    storageError: 'Lỗi lưu trữ thử nghiệm' }));
  await page.getByRole('alert').filter({ hasText: 'Lỗi lưu trữ thử nghiệm' }).waitFor();
  await assertLayout('overlay diagnostic');
  const overlaps = await page.locator('.meter').evaluate(element => {
    const controls = element.querySelector('.meter-controls').getBoundingClientRect();
    return [...element.querySelectorAll('.meter-item')].some(item => {
      const rect = item.getBoundingClientRect();
      return rect.left < controls.right && rect.right > controls.left && rect.top < controls.bottom && rect.bottom > controls.top;
    });
  });
  assert.equal(overlaps, false, 'Overlay metrics overlap controls');
  await page.screenshot({ path: path.join(output, 'overlay.png') });
  await page.getByRole('button', { name: 'Ẩn widget', exact: true }).click();
  await page.waitForFunction(() => !document.querySelector('.meter'));
  assert.equal(await page.evaluate(() => window.__testCalls.filter(call => call.command === 'begin_overlay_drag').length), drags, 'Close control began a drag');
  assert.deepEqual(errors, [], 'Browser runtime/asset errors');
  console.log(JSON.stringify({ status: 'passed', viewports: [1366, 780, 420], overlay: '336x164',
    assets, nativeIPC: 'mocked, no native kill/settings actions', server: 'none', screenshots: output }));
} catch (error) {
  await page.screenshot({ path: path.join(output, 'failure.png'), fullPage: true }).catch(() => {});
  throw error;
} finally {
  await browser.close();
}
