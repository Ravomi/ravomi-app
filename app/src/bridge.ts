import { invoke, isTauri } from '@tauri-apps/api/core';

export const native = isTauri();

export interface GraphNode {
  id: string;
  kind: string;
  text: string;
  rect: [number, number, number, number];
  editable: boolean;
}

export interface Snapshot {
  projectId: string;
  documentToken: string;
  archivedWorkflowCount: number;
  title: string;
  path: string | null;
  dirty: boolean;
  canUndo: boolean;
  canRedo: boolean;
  nodes: GraphNode[];
  edgeCount: number;
}

export type Command =
  | { type: 'new'; discard: boolean }
  | { type: 'open'; path: string; discard: boolean }
  | { type: 'save'; path: string }
  | { type: 'add_text'; x: number; y: number }
  | { type: 'edit_text'; id: string; text: string }
  | { type: 'undo' | 'redo' };

// Browser storage/WASM will implement this boundary in a later migration step.
// Never pretend a browser-only preview has a working Rust backend.
export const getSnapshot = () => invoke<Snapshot>('session_snapshot');
export const execute = (command: Command) => invoke<Snapshot>('session_execute', { command });
