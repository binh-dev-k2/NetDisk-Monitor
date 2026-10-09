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
    usedMemoryBytes: 4000000000, storageError: null, taskbarError: null, processNetworkError: null, processes };
  let overlayVisible = true;
  let taskbarMode = false;
  let pinned = true;
  const emit = (event, payload) => {
    for (const [eventId, listener] of listeners) {
      if (listener.event === event) callbacks.get(listener.handler)?.({ event, id: eventId, payload });
    }
  };
  window.__testCalls = [];
  window.__testEmit = emit;
  window.__testProcesses = () => snapshot.processes;
  window.__testProcessUpdate = change => {
    snapshot = { ...snapshot, ...change, processSequence: snapshot.processSequence + 1 };
    emit('processes://snapshot', { processSequence: snapshot.processSequence,
      processes: snapshot.processes, processNetworkError: snapshot.processNetworkError });
  };
  window.__TAURI_INTERNALS__ = {
    transformCallback(callback) { callbacks.set(++id, callback); return id; },
    async invoke(command, args = {}) {
      window.__testCalls.push({ command, args });
      if (command === 'plugin:event|listen') { listeners.set(++id, args); return id; }
      if (command === 'plugin:event|unlisten') { listeners.delete(args.eventId); return; }
      if (command === 'get_snapshot') return snapshot;
      if (command === 'get_overlay_settings') return { ...settings };
      if (command === 'update_overlay_settings') {
        Object.assign(settings, args.settings); emit('overlay://settings', { ...settings }); return { ...settings };
      }
      if (command === 'get_overlay_visible') return overlayVisible;
      if (command === 'get_overlay_pinned') return pinned;
      if (command === 'get_taskbar_mode') return taskbarMode;
      if (command === 'get_autostart') return false;
      if (command === 'set_taskbar_mode') { taskbarMode = args.enabled; emit('taskbar://mode', taskbarMode); return taskbarMode; }
      if (command === 'set_overlay_visible') { overlayVisible = args.visible; emit('overlay://visibility', overlayVisible); return overlayVisible; }
      if (command === 'set_overlay_pinned') { pinned = args.pinned; emit('overlay://pinned', pinned); return pinned; }
      if (command === 'set_autostart') return args.enabled;
      if (command === 'export_csv_file') return 'C:/Users/Test/Downloads/' + args.filename;
      if (command === 'get_history') return Array.from({ length: 30 }, (_, index) => ({
        timestampSecs: 1700000000 - index * (args.resolution === 'day' ? 86400 : 60), durationMillis: 60000, bytes: [600000, 100000, 400000, 200000],
      }));
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

try {
  for (const width of [1366, 780, 420]) {
    await page.setViewportSize({ width, height: 800 });
    await page.goto('http://monitor.test/');
    await page.getByRole('button', { name: 'Theo Process', exact: true }).click();
    const row = page.locator('.process-row.parent').filter({ has: page.getByText('network-regression.exe', { exact: true }) });
    await row.waitFor();
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

    await page.getByRole('button', { name: 'Tổng quan', exact: true }).click();
    await page.locator('.metric-grid').waitFor();
    await page.locator('.history-scroll tbody tr').first().waitFor();
    await assertLayout(`${width} overview`);
    await page.screenshot({ path: path.join(output, `overview-${width}.png`), fullPage: true });
    await page.getByRole('button', { name: 'Trang lịch sử sau', exact: true }).click();
    await page.getByRole('combobox', { name: 'Độ chi tiết lịch sử' }).selectOption('minute');
    await page.locator('.history-scroll tbody tr').first().waitFor();
    await page.getByRole('button', { name: 'Làm mới lịch sử', exact: true }).click();
    await page.locator('.history-scroll tbody tr').first().waitFor();
    await page.getByRole('button', { name: 'Xuất lịch sử CSV', exact: true }).click();
    await page.getByRole('status').filter({ hasText: 'Đã lưu CSV:' }).waitFor();
    assert(await page.evaluate(() => window.__testCalls.some(call => call.command === 'export_csv_file' && call.args.content.includes('NetworkDownBytes'))), 'History CSV is not exported');
    await page.getByRole('checkbox', { name: 'Khởi động cùng hệ thống' }).check();
    await page.waitForFunction(() => document.querySelector('.autostart-toggle input').checked);
    await page.getByRole('combobox', { name: 'Chế độ widget' }).selectOption('taskbar');
    await page.waitForFunction(() => document.querySelector('.widget-mode-select').value === 'taskbar');
    assert(await page.getByRole('slider', { name: 'Độ mờ thanh nổi' }).isDisabled());
    await page.getByRole('combobox', { name: 'Chế độ widget' }).selectOption('hidden');
    await page.waitForFunction(() => document.querySelector('.widget-mode-select').value === 'hidden');
    await page.getByRole('combobox', { name: 'Chế độ widget' }).selectOption('overlay');
    await page.waitForFunction(() => document.querySelector('.widget-mode-select').value === 'overlay');
    await page.getByRole('slider', { name: 'Độ mờ thanh nổi' }).fill('60');
    await page.getByRole('checkbox', { name: 'Tốc độ tải xuống', exact: true }).uncheck();
    await page.getByRole('checkbox', { name: 'Tốc độ tải xuống', exact: true }).check();
    await page.getByRole('button', { name: 'Reset Phiên', exact: true }).click();
    assert(await page.evaluate(() => window.__testCalls.some(call => call.command === 'reset_session_totals')));
    await assertLayout(`${width} settings`);
    await page.screenshot({ path: path.join(output, `settings-${width}.png`) });
    console.log(`viewport ${width}: totals/nulls, realtime memo, diagnostic, exact kill IPC, CSV, controls and layout passed`);
  }

  await page.setViewportSize({ width: 336, height: 164 });
  await page.goto('http://monitor.test/meter.html');
  await page.locator('.meter-item').first().waitFor();
  await assertLayout('overlay 336x164');
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
