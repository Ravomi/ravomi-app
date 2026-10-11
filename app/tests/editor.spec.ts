import { expect, test, type Page } from '@playwright/test';

// Deliberately mocked IPC: these tests cover UI lifecycle and payloads, while
// src/session.rs tests the real Rust document/history/PRG implementation.
async function desktop(page: Page) {
  await page.addInitScript(() => {
    const testWindow = window as typeof window & {
      isTauri: boolean;
      __TAURI_INTERNALS__: object;
      __TAURI_EVENT_PLUGIN_INTERNALS__: object;
      commands: unknown[];
    };
    testWindow.isTauri = true;
    testWindow.commands = [];
    const snapshot = {
      title: '很长的中文文件名——项目探索与想法整理.prg', path: '/test.prg',
      dirty: false, canUndo: false, canRedo: false,
      nodes: [{ id: 'node-1', kind: 'TextNode', text: '原始想法', rect: [0, 0, 100, 50], editable: true }],
      edgeCount: 0,
    };
    testWindow.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    testWindow.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: 'main' } },
      transformCallback() { return 1; },
      async invoke(name: string, args: { command?: { type: string; text?: string } }) {
        if (name === 'session_snapshot') return structuredClone(snapshot);
        if (name === 'plugin:event|listen') return 1;
        if (name === 'plugin:event|unlisten') return;
        if (name === 'plugin:dialog|message') return false;
        if (name === 'session_execute') {
          testWindow.commands.push(args.command);
          if (args.command?.type === 'edit_text') {
            snapshot.nodes[0].text = args.command.text!;
            snapshot.dirty = true;
            snapshot.canUndo = true;
          } else if (args.command?.type === 'save') {
            throw new Error('磁盘文件已被其他程序更改，请另存为以保留两个版本');
          }
          return structuredClone(snapshot);
        }
        throw new Error(`Unexpected IPC: ${name}`);
      },
    };
  });
  await page.goto('/');
  await expect(page.getByRole('button', { name: '新建文字', exact: true })).toBeEnabled();
}

test('browser preview explicitly disables document operations', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('当前为浏览器预览。', { exact: false })).toBeVisible();
  for (const button of await page.getByRole('button', { name: '新建文字', exact: true }).all()) {
    await expect(button).toBeDisabled();
  }
  await expect(page.locator('.app')).toHaveAttribute('data-theme', 'light');
  await page.getByRole('button', { name: '切换深色主题' }).click();
  await expect(page.locator('.app')).toHaveAttribute('data-theme', 'dark');
});

test('saving applies a draft, shows IO errors, and preserves the edited text', async ({ page }) => {
  await desktop(page);
  await page.getByRole('button', { name: /原始想法/ }).click();
  await page.getByLabel('文字内容').fill('新的中文想法');
  await page.getByLabel('工作区菜单').click();
  await page.getByRole('button', { name: '保存', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('磁盘文件已被其他程序更改');
  await expect(page.getByLabel('文字内容')).toHaveValue('新的中文想法');
  await expect(page.locator('.document-tab')).toContainText('未保存');
  expect(await page.evaluate(() => (window as unknown as { commands: unknown[] }).commands)).toEqual([
    { type: 'edit_text', id: 'node-1', text: '新的中文想法' },
    { type: 'save', path: '/test.prg' },
  ]);
});

test('canceling replacement preserves unapplied text', async ({ page }) => {
  await desktop(page);
  await page.getByRole('button', { name: /原始想法/ }).click();
  await page.getByLabel('文字内容').fill('不要丢失');
  await page.getByLabel('工作区菜单').click();
  await page.getByRole('button', { name: '新建画布', exact: true }).click();
  await expect(page.getByLabel('文字内容')).toHaveValue('不要丢失');
  expect(await page.evaluate(() => (window as unknown as { commands: unknown[] }).commands)).toEqual([]);
});

for (const theme of ['dark', 'light']) {
  for (const scale of ['100', '150', '200']) {
    test(`layout ${theme} ${scale}%`, async ({ page }, testInfo) => {
      await desktop(page);
      await page.getByRole('button', { name: /原始想法/ }).click();
      if (theme === 'dark') await page.getByRole('button', { name: '切换深色主题' }).click();
      await page.getByLabel('界面缩放').selectOption(scale);
      await expect(page.getByLabel('文字内容')).toBeVisible();
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
      await page.screenshot({ path: testInfo.outputPath('desktop.png'), fullPage: true });
      await page.setViewportSize({ width: 390, height: 844 });
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
      // Page scroll width alone misses controls clipped by a rounded panel.
      for (const selector of ['.workspace-heading button', '.inspector textarea']) {
        expect(await page.locator(selector).evaluate((element) => {
          const bounds = element.getBoundingClientRect();
          const panel = element.closest('.workspace-heading, .inspector')!.getBoundingClientRect();
          return bounds.left >= panel.left && bounds.right <= panel.right && bounds.right <= window.innerWidth;
        })).toBe(true);
      }
      await page.screenshot({ path: testInfo.outputPath('narrow.png'), fullPage: true });
    });
  }
}

test('brand assets load and keyboard focus survives a theme change with reduced motion', async ({ page }) => {
  await desktop(page);
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await expect(page.locator('.hero-avatar')).toBeVisible();
  expect(await page.locator('.hero-avatar').evaluate((image) => (image as HTMLImageElement).naturalWidth)).toBeGreaterThan(0);
  const theme = page.getByRole('button', { name: '切换深色主题' });
  await theme.focus();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('button', { name: '切换浅色主题' })).toBeFocused();
  expect(await page.locator('.window-actions button').first().evaluate((button) => getComputedStyle(button).transitionDuration)).toBe('0s');
  await page.keyboard.press('Tab');
  await expect(page.getByLabel('界面缩放')).toBeFocused();
});
