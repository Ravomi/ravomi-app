import { test } from 'node:test';
import assert from 'node:assert/strict';
import { delegationTools, settleTool, abortTools } from '../src/delegation.ts';
import { Agent } from '@earendil-works/pi-agent-core';
import { createAssistantMessageEventStream, createModels } from '@earendil-works/pi-ai';
import { deepseekProvider } from '@earendil-works/pi-ai/providers/deepseek';

test('delegation routes concurrent requests by call identity and preserves errors', async () => {
  const events = [];
  const tools = delegationTools(value => events.push(value));
  const first = tools[0].execute('a', { title: 'A', instruction: 'a', acceptance: 'a' });
  const second = tools[1].execute('b', { run_id: 'child' });
  assert.deepEqual(events.map(e => e.name), ['spawn_agent', 'wait_agent']);
  settleTool({ id: 'b', value: { status: 'succeeded' } });
  assert.match((await second).content[0].text, /succeeded/);
  settleTool({ id: 'a', error: '派发已暂停' });
  await assert.rejects(first, /派发已暂停/);
});

test('cancellation rejects pending tools and late replies cannot revive them', async () => {
  const tools = delegationTools(() => {});
  const controller = new AbortController();
  const waiting = tools[1].execute('cancel', { run_id: 'child' }, controller.signal);
  controller.abort();
  await assert.rejects(waiting, /取消/);
  settleTool({ id: 'cancel', value: {} });
  const pending = tools[0].execute('shutdown', {});
  abortTools();
  await assert.rejects(pending, /取消/);
});

test('real Pi loop continues through spawn and wait tools to a final answer with a controlled stream', async () => {
  const models = createModels();
  models.setProvider(deepseekProvider());
  const model = models.getModel('deepseek', 'deepseek-flash');
  const calls = [];
  let turn = 0;
  const agent = new Agent({
    initialState: { model, thinkingLevel: 'off', tools: delegationTools(event => {
      calls.push(event.name);
      settleTool({ id: event.id, value: event.name === 'spawn_agent' ? { run_id: 'child' } : { status: 'succeeded', proposal: { summary: 'child result', files: [] } } });
    }) },
    streamFn: (_model, context) => {
      const stream = createAssistantMessageEventStream();
      const content = turn === 0
        ? [{ type: 'toolCall', id: 'spawn', name: 'spawn_agent', arguments: { title: 'test', instruction: 'test', acceptance: 'test' } }]
        : turn === 1 ? [{ type: 'toolCall', id: 'wait', name: 'wait_agent', arguments: { run_id: 'child' } }]
        : [{ type: 'text', text: '{"summary":"combined","files":[]}' }];
      if (turn === 2) assert.ok(context.messages.some(m => m.role === 'toolResult' && m.toolCallId === 'wait'));
      const message = { role: 'assistant', content, api: model.api, provider: model.provider, model: model.id,
        usage: { input: 1, output: 1, cacheRead: 0, cacheWrite: 0, totalTokens: 2, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } },
        stopReason: turn++ < 2 ? 'toolUse' : 'stop', timestamp: Date.now() };
      stream.push({ type: 'done', reason: message.stopReason, message });
      return stream;
    },
  });
  await agent.prompt('Delegate and combine.');
  assert.equal(turn, 3);
  assert.deepEqual(calls, ['spawn_agent', 'wait_agent']);
  assert.match(agent.state.messages.at(-1).content[0].text, /combined/);
});
