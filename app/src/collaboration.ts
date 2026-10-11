import { invoke } from '@tauri-apps/api/core';
import type { Snapshot } from './bridge';

export interface SourceFile { path: string; content: string }
export interface Task { id: string; title: string; role: string; instruction: string; acceptance: string; dependencies: string[]; parentAgent: string | null }
export interface Plan { tasks: Task[] }
export type RunStatus = 'queued' | 'running' | 'stopping' | 'succeeded' | 'failed' | 'cancelled' | 'unknown' | 'storage_blocked';
export interface Run { id: string; taskId: string; status: RunStatus; createdAt: number; finishedAt: number | null; sources: string[]; prompt: string; inputFiles: SourceFile[]; output: string; proposal: { summary: string; files: SourceFile[] } | null; error: string | null; review: string | null; usage: Record<string, unknown> | null; sessionId: string | null; retryOf: string | null }
export interface Workflow { id: string; goal: string; project: string; model: string; files: SourceFile[]; testCommand: string[]; plan: Plan | null; draftPlan: Plan | null; runs: Run[]; paused: boolean; maxCalls: number; timeoutSeconds: number }
export interface CollaborationSnapshot { version: number; workflows: Workflow[]; storageError: string | null }
export interface ArchivedWorkflow extends Pick<Workflow, 'id' | 'goal' | 'model' | 'files' | 'plan' | 'draftPlan'> { runs: Omit<Run, 'sessionId'>[] }
export interface ProjectHistory { schemaVersion: number; workflows: ArchivedWorkflow[] }
export const archiveProjectWorkflow = (projectId: string, documentToken: string, workflowId: string) => invoke<Snapshot>('archive_project_workflow', { projectId, documentToken, workflowId });
export const projectArchivedHistory = (projectId: string, documentToken: string) => invoke<ProjectHistory>('project_archived_history', { projectId, documentToken });
export const aiSnapshot = () => invoke<CollaborationSnapshot>('ai_snapshot');
export const aiCommand = (command: object) => invoke<CollaborationSnapshot>('ai_command', { command });
export const aiScan = (project: string) => invoke<string[]>('ai_scan', { project });
export const aiExport = (workflowId: string, runId: string, destination: string) => invoke<string>('ai_export', { workflowId, runId, destination });
export const aiHasActive = () => invoke<boolean>('ai_has_active');
export const aiShutdown = () => invoke<void>('ai_shutdown');
export const aiConfigure = (provider: string, apiKey: string) => invoke<void>('ai_configure', { provider, apiKey });
