import { expect, test } from '@playwright/test';

// UI contract tests only; real scheduling and disk recovery are tested in Rust.
test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    const host = window as typeof window & { isTauri: boolean; __TAURI_INTERNALS__: object; __TAURI_EVENT_PLUGIN_INTERNALS__: object; commands: object[] };
    host.isTauri = true;
    host.commands = [];
    const task = { id: 'code', title: '实现加法', role: '开发', instruction: '编写加法', acceptance: '通过测试', dependencies: [], parentAgent: null };
    const workflow = { id: 'w1', goal: '实现求和', project: '/code', model: 'openai/test', files: [{ path: 'math.js', content: '// baseline' }], testCommand: ['node', '--test'], plan: null as object | null, draftPlan: { tasks: [task] }, runs: [{ id: 'r1', taskId: 'code', status: 'succeeded', createdAt: 1, finishedAt: 2, sources: [], prompt: 'fixed input', inputFiles: [], output: '', proposal: { summary: '新增加法', files: [{ path: 'math.js', content: 'export const add = (a,b) => a+b;' }] }, error: null, review: null as string | null, usage: null, sessionId: null, retryOf: null }], paused: true, maxCalls: 16, timeoutSeconds: 180 };
    const snapshot = { version: 1, workflows: [] as typeof workflow[], storageError: null };
    host.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    host.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: 'main' } }, transformCallback() { return 1; },
      async invoke(name: string, args: { command?: { type: string; decision?: string }; provider?: string; apiKey?: string }) {
        if (name === 'session_snapshot') return { title: '未命名', path: null, dirty: false, nodes: [], edgeCount: 0, canUndo: false, canRedo: false };
        if (name === 'plugin:event|listen') return 1;
        if (name === 'plugin:event|unlisten') return;
        if (name === 'plugin:dialog|open') return '/code';
        if (name === 'ai_scan') return ['math.js', 'private.js'];
        if (name === 'ai_snapshot') return structuredClone(snapshot);
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
