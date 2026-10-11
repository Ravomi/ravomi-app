import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    const host = window as typeof window & { messages: object[]; ipc: { postMessage(text: string): void } };
    host.messages = [];
    host.ipc = { postMessage(text) { host.messages.push(JSON.parse(text)); } };
  });
  await page.goto('/canvas-editor.html');
  await page.waitForFunction(() => Boolean((window as unknown as { setCanvasEdit: unknown }).setCanvasEdit));
  await page.evaluate(() => (window as unknown as { setCanvasEdit(value: object): void }).setCanvasEdit({
    session: 1, text: '', fontSize: 32, contentWidth: 240, autoSize: true, light: false,
  }));
});

test('preedit stays transient and Enter commits only after composition ends', async ({ page }) => {
  const input = page.getByRole('textbox', { name: '节点文字' });
  await input.dispatchEvent('compositionstart', { data: '' });
  await input.fill('ni hao');
  await input.press('Enter');
  expect(await page.evaluate(() => (window as unknown as { messages: { type: string }[] }).messages.filter(m => m.type === 'commit'))).toEqual([]);
  await input.fill('你好');
  await input.dispatchEvent('compositionend', { data: '你好' });
  await input.press('Enter');
  const messages = await page.evaluate(() => (window as unknown as { messages: { type: string; text?: string; composing?: boolean; width?: number }[] }).messages);
  expect(messages.some(m => m.type === 'draft' && m.text === 'ni hao' && m.composing)).toBe(true);
  expect(messages.filter(m => m.type === 'commit')).toMatchObject([{ session: 1, text: '你好' }]);
  expect(messages.at(-1)?.width).toBeGreaterThan(0);
});

test('host resize preserves draft and caret; Shift Enter inserts a newline', async ({ page }) => {
  const input = page.getByRole('textbox', { name: '节点文字' });
  await input.fill('ab');
  await input.press('ArrowLeft');
  await page.evaluate(() => (window as unknown as { setCanvasEdit(value: object): void }).setCanvasEdit({
    session: 1, text: '', fontSize: 48, contentWidth: 400, autoSize: true, light: true,
  }));
  await input.press('Shift+Enter');
  await expect(input).toHaveValue('a\nb');
  expect(await page.evaluate(() => (window as unknown as { messages: { type: string }[] }).messages.filter(m => m.type === 'commit'))).toEqual([]);
});

for (const light of [false, true]) {
  for (const scale of [1, 1.5, 2]) {
    test(`layout ${light ? 'light' : 'dark'} ${scale}`, async ({ page }, testInfo) => {
      await page.setViewportSize({ width: 420, height: 240 });
      await page.evaluate(({ light, scale }) => {
        // Match the canvas surrounding the native child WebView.
        document.body.style.background = light ? '#eff1f5' : '#313244';
        (window as unknown as { setCanvasEdit(value: object): void }).setCanvasEdit({
          session: 2, text: '你好\n文字编辑', fontSize: 32 * scale, contentWidth: 400, autoSize: false, light,
        });
      }, { light, scale });
      const input = page.getByRole('textbox', { name: '节点文字' });
      await expect(input).toHaveValue('你好\n文字编辑');
      expect(await input.evaluate(el => el.scrollWidth <= el.clientWidth + 1)).toBe(true);
      await page.screenshot({ path: testInfo.outputPath('editor.png') });
    });
  }
}
