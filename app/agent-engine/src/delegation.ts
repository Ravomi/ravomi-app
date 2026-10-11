import type { AgentTool } from '@earendil-works/pi-agent-core';
import { Type } from 'typebox';

const pending = new Map<string, { resolve: (value: unknown) => void; reject: (error: Error) => void }>();

export function settleTool(record: { id?: string; value?: unknown; error?: string }) {
  const waiter = record.id ? pending.get(record.id) : undefined;
  if (!waiter) return;
  pending.delete(record.id!);
  if (record.error) waiter.reject(new Error(record.error));
  else waiter.resolve(record.value);
}

export function abortTools() {
  for (const waiter of pending.values()) waiter.reject(new Error('运行已取消'));
  pending.clear();
}

export function delegationTools(emit: (value: unknown) => unknown): AgentTool[] {
  const definitions = [
    { name: 'spawn_agent', description: 'Delegate an independent subtask to a child with the same model and fixed input files. Returns a run_id immediately. No command execution or nested delegation.', parameters: Type.Object({ title: Type.String({ minLength: 1, maxLength: 200 }), instruction: Type.String({ minLength: 1, maxLength: 16000 }), acceptance: Type.String({ minLength: 1, maxLength: 4000 }) }) },
    { name: 'wait_agent', description: 'Wait for one child created by this run. Returns its saved status, proposal and error. Inspect status before using the result.', parameters: Type.Object({ run_id: Type.String() }) },
  ];
  return definitions.map(definition => ({
    ...definition, label: definition.name,
    execute: async (id, args, signal) => {
      if (signal?.aborted) throw new Error('运行已取消');
      if (pending.has(id)) throw new Error('重复的在途工具调用');
      const abort = () => { pending.get(id)?.reject(new Error('运行已取消')); pending.delete(id); };
      try {
        const value = await new Promise<unknown>((resolve, reject) => {
          pending.set(id, { resolve, reject });
          signal?.addEventListener('abort', abort, { once: true });
          emit({ type: 'tool_request', id, name: definition.name, arguments: args });
        });
        return { content: [{ type: 'text' as const, text: JSON.stringify(value) }], details: undefined };
      } finally { pending.delete(id); signal?.removeEventListener('abort', abort); }
    },
  }));
}
