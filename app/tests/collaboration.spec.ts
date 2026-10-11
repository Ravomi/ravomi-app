import { expect, test } from '@playwright/test';

// UI contract tests only; real scheduling and disk recovery are tested in Rust.
test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    const host = window as typeof window & { isTauri: boolean; __TAURI_INTERNALS__: object; __TAURI_EVENT_PLUGIN_INTERNALS__: object; commands: object[]; failArchive: boolean; failHistory: boolean };
    host.isTauri = true;
    host.commands = [];
    host.failArchive = false;
    host.failHistory = false;
    const task = { id: 'code', title: '实现加法', role: '开发', instruction: '编写加法', acceptance: '通过测试', dependencies: [], parentAgent: null };
    const workflow = { id: 'w1', goal: '实现求和', project: '/code', model: 'openai/test', files: [{ path: 'math.js', content: '// baseline' }], testCommand: ['node', '--test'], plan: null as object | null, draftPlan: { tasks: [task] }, runs: [{ id: 'r1', taskId: 'code', status: 'succeeded', createdAt: 1, finishedAt: 2, sources: [], prompt: 'fixed input', inputFiles: [], output: '', proposal: { summary: '新增加法', files: [{ path: 'math.js', content: 'export const add = (a,b) => a+b;' }] }, error: null, review: null as string | null, usage: null, sessionId: null, retryOf: null }], paused: true, maxCalls: 16, timeoutSeconds: 180 };
    const snapshot = { version: 1, workflows: [] as typeof workflow[], storageError: null };
    const document = { projectId: 'project-1', documentToken: 'document-1', archivedWorkflowCount: 0, title: '未命名', path: null as string | null, dirty: false, nodes: [], edgeCount: 0, canUndo: false, canRedo: false };
    let documentGeneration = 1;
    const history = { schemaVersion: 1, workflows: [] as object[] };
    host.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    host.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: 'main' } }, transformCallback() { return 1; },
      async invoke(name: string, args: { command?: { type: string; decision?: string }; provider?: string; apiKey?: string; projectId?: string; documentToken?: string; workflowId?: string }) {
        if (name === 'session_snapshot') return structuredClone(document);
        if (name === 'session_execute') {
          host.commands.push(args.command!);
          if (args.command?.type === 'save') { document.path = '/saved.mio'; document.title = 'saved.mio'; document.dirty = false; }
          if (args.command?.type === 'open') { snapshot.workflows = []; document.documentToken = `document-${++documentGeneration}`; document.path = '/saved.mio'; document.title = 'saved.mio'; document.dirty = false; }
          return structuredClone(document);
        }
        if (name === 'plugin:event|listen') return 1;
        if (name === 'plugin:event|unlisten') return;
        if (name === 'plugin:dialog|open') return '/code';
        if (name === 'plugin:dialog|save') return '/saved.mio';
        if (name === 'ai_scan') return ['math.js', 'private.js'];
        if (name === 'ai_snapshot') return structuredClone(snapshot);
        if (name === 'archive_project_workflow') {
          host.commands.push({ type: 'archive', projectId: args.projectId, documentToken: args.documentToken, workflowId: args.workflowId });
          if (host.failArchive || args.projectId !== document.projectId || args.documentToken !== document.documentToken) throw new Error('当前项目已切换，请在目标项目中重新操作');
          const selected = snapshot.workflows.find(item => item.id === args.workflowId)!;
          const { id, goal, model, files, plan, draftPlan } = selected;
          history.workflows = [{ id, goal, model, files, plan, draftPlan, runs: selected.runs.map(({ sessionId: _sessionId, ...run }) => run) }];
          document.archivedWorkflowCount = history.workflows.length;
          document.dirty = true;
          return structuredClone(document);
        }
        if (name === 'project_archived_history') {
          host.commands.push({ type: 'read_archive', projectId: args.projectId, documentToken: args.documentToken });
          if (args.projectId !== document.projectId || args.documentToken !== document.documentToken) throw new Error('当前项目已切换，请在目标项目中重新操作');
          if (host.failHistory) throw new Error('暂时无法读取历史');
          return structuredClone(history);
        }
        if (name === 'ai_configure') { host.commands.push({ type: 'configure', provider: args.provider, hasKey: Boolean(args.apiKey) }); return; }
        if (name === 'ai_command') {
          host.commands.push(args.command!);
          if (args.command?.type === 'create') snapshot.workflows = [workflow];
          if (args.command?.type === 'start') workflow.paused = false;
          if (args.command?.type === 'review') workflow.runs[0].review = args.command.decision!;
          return structuredClone(snapshot);
        }
        if (name === 'ai_export') return '/export/ravomi-r1';
        throw new Error(`Unexpected IPC: ${name}`);
      },
    };
  });
  await page.goto('/');
  await page.getByRole('button', { name: '打开代码协作' }).click();
});

test('only explicitly checked sources are sent and approval gates export', async ({ page }) => {
  await page.getByLabel('这次要完成什么？').fill('实现求和');
  await page.getByRole('button', { name: '选择代码目录' }).click();
  await expect(page.getByRole('button', { name: '生成协作计划' })).toBeDisabled();
  await page.getByLabel('math.js', { exact: true }).check();
  await page.getByRole('button', { name: '生成协作计划' }).click();
  expect(await page.evaluate(() => (window as unknown as { commands: object[] }).commands)).toContainEqual(expect.objectContaining({ type: 'create', paths: ['math.js'] }));
  await page.getByText('运行历史与测试').click();
  await page.getByRole('button', { name: /code · 已生成/ }).click();
  await expect(page.getByRole('button', { name: '导出代码快照' })).toBeDisabled();
  await page.getByRole('button', { name: '审阅通过', exact: true }).click();
  await expect(page.getByRole('button', { name: '导出代码快照' })).toBeEnabled();
  await page.getByRole('button', { name: '返回画布' }).click();
  await expect(page.getByRole('button', { name: '新建文字', exact: true })).toBeEnabled();
});

test('session credential entry clears secret after applying', async ({ page }) => {
  await page.getByText('模型连接设置', { exact: true }).click();
  await page.getByLabel('模型 API Key').fill('test-placeholder-not-a-real-secret');
  await page.getByRole('button', { name: '应用到本次会话' }).click();
  await expect(page.getByLabel('模型 API Key')).toHaveValue('');
  await expect(page.getByRole('status').filter({ hasText: '尚未验证网络调用' })).toBeVisible();
});

test('narrow collaboration form remains readable', async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 420, height: 900 });
  await expect(page.getByLabel('这次要完成什么？')).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath('code-collaboration.png'), fullPage: true });
});

test('explicit capture updates project dirty state and reopens as read-only history', async ({ page }) => {
  await page.getByLabel('这次要完成什么？').fill('实现求和');
  await page.getByRole('button', { name: '选择代码目录' }).click();
  await page.getByLabel('math.js', { exact: true }).check();
  await page.getByRole('button', { name: '生成协作计划' }).click();
  expect(await page.evaluate(() => (window as unknown as { commands: object[] }).commands)).not.toContainEqual(expect.objectContaining({ type: 'archive' }));
  await page.getByRole('button', { name: '保存到当前项目', exact: true }).click();
  expect(await page.evaluate(() => (window as unknown as { commands: object[] }).commands)).toContainEqual({ type: 'archive', projectId: 'project-1', documentToken: 'document-1', workflowId: 'w1' });
  await expect(page.getByText('未命名 · 未保存', { exact: true })).toBeVisible();
  await expect(page.getByRole('status').filter({ hasText: '保存 .mio 项目文件后' })).toBeVisible();
  await page.getByRole('button', { name: '保存画布', exact: true }).click();
  await page.getByRole('button', { name: '打开文件', exact: true }).click();
  const archive = page.getByLabel('已保存协作历史', { exact: true });
  await archive.locator('summary').first().click();
  await expect(archive).toContainText('已保存协作历史（1）');
  await archive.getByText('实现求和 · 1 次运行', { exact: true }).click();
  await expect(archive.locator('pre')).toContainText('fixed input');
  await expect(archive.locator('pre')).toContainText('export const add');
  await expect(archive.locator('pre')).not.toContainText('sessionId');
  await expect(archive.getByRole('button')).toHaveCount(0);
  expect(await page.evaluate(() => (window as unknown as { commands: object[] }).commands)).not.toContainEqual(expect.objectContaining({ type: 'start' }));
});

test('project history reports read failures and supports retry', async ({ page }) => {
  await page.evaluate(() => { (window as unknown as { failHistory: boolean }).failHistory = true; });
  const archive = page.getByLabel('已保存协作历史', { exact: true });
  await archive.locator('summary').first().click();
  await expect(archive.getByRole('alert')).toContainText('暂时无法读取历史');
  await page.evaluate(() => { (window as unknown as { failHistory: boolean }).failHistory = false; });
  await archive.getByRole('button', { name: '重新读取历史' }).click();
  await expect(archive).toContainText('尚未向此项目保存协作历史');
});

test('opening a copy with the same project identity reloads its own history', async ({ page }) => {
  const archive = page.getByLabel('已保存协作历史', { exact: true });
  await archive.locator('summary').first().click();
  await expect(archive).toContainText('尚未向此项目保存协作历史');
  expect(await page.evaluate(() => (window as unknown as { commands: object[] }).commands)).toContainEqual({ type: 'read_archive', projectId: 'project-1', documentToken: 'document-1' });
  await page.getByRole('button', { name: '打开文件', exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as unknown as { commands: object[] }).commands)).toContainEqual({ type: 'read_archive', projectId: 'project-1', documentToken: 'document-2' });
  await expect(archive).toContainText('当前项目：saved.mio');
  await expect(archive.getByRole('alert')).toHaveCount(0);
});

test('a rejected project identity does not mark the current project saved or dirty', async ({ page }) => {
  await page.getByLabel('这次要完成什么？').fill('实现求和');
  await page.getByRole('button', { name: '选择代码目录' }).click();
  await page.getByLabel('math.js', { exact: true }).check();
  await page.getByRole('button', { name: '生成协作计划' }).click();
  await page.evaluate(() => { (window as unknown as { failArchive: boolean }).failArchive = true; });
  await page.getByRole('button', { name: '保存到当前项目', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('当前项目已切换');
  await expect(page.getByText('未命名 · 未保存', { exact: true })).toHaveCount(0);
  await expect(page.getByRole('status').filter({ hasText: '已将此流程' })).toHaveCount(0);
});

test('archived evidence remains readable across themes and scales', async ({ page }, testInfo) => {
  await page.getByLabel('这次要完成什么？').fill('实现求和');
  await page.getByRole('button', { name: '选择代码目录' }).click();
  await page.getByLabel('math.js', { exact: true }).check();
  await page.getByRole('button', { name: '生成协作计划' }).click();
  await page.getByRole('button', { name: '保存到当前项目', exact: true }).click();
  const archive = page.getByLabel('已保存协作历史', { exact: true });
  await archive.locator('summary').first().focus();
  await page.keyboard.press('Enter');
  await archive.getByText('实现求和 · 1 次运行', { exact: true }).focus();
  await page.keyboard.press('Enter');
  for (const theme of ['light', 'dark']) {
    if (theme === 'dark') await page.getByRole('button', { name: '切换深色主题' }).click();
    for (const scale of ['100', '150', '200']) {
      await page.getByLabel('界面缩放').selectOption(scale);
      await expect(archive.locator('pre')).toContainText('fixed input');
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
      await page.screenshot({ path: testInfo.outputPath(`archive-${theme}-${scale}.png`), fullPage: true });
    }
  }
  await page.getByLabel('界面缩放').selectOption('100');
  await page.setViewportSize({ width: 420, height: 900 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath('archive-narrow.png'), fullPage: true });
});
