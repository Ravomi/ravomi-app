import { useEffect, useRef, useState } from 'react';
import { ask, open } from '@tauri-apps/plugin-dialog';
import { native, type Snapshot } from './bridge';
import { aiSnapshot, aiCommand, aiScan, aiExport, aiConfigure, archiveProjectWorkflow, projectArchivedHistory, type CollaborationSnapshot, type ProjectHistory, type Run, type RunStatus, type Workflow } from './collaboration';
import { Icon } from './Icon';
import './collaboration.css';

const labels: Record<RunStatus, string> = { queued: '排队', running: '运行中', stopping: '停止中', succeeded: '已生成', failed: '失败', cancelled: '已停止', unknown: '待核对', storage_blocked: '产物待保存' };
const last = (w: Workflow, task: string) => [...w.runs].reverse().find(r => r.taskId === task);

interface Props {
  currentProject: Snapshot;
  projectBusy: boolean;
  runProjectAction: (operation: () => Promise<void>) => Promise<void>;
  onProjectChange: (snapshot: Snapshot) => void;
}

export function Collaboration({ currentProject, projectBusy, runProjectAction, onProjectChange }: Props) {
  const [snapshot, setSnapshot] = useState<CollaborationSnapshot>({ version: 1, workflows: [], storageError: null });
  const [workflowId, setWorkflowId] = useState('');
  const [project, setProject] = useState('');
  const [paths, setPaths] = useState<string[]>([]);
  const [selected, setSelected] = useState<string[]>([]);
  const [goal, setGoal] = useState('');
  const [model, setModel] = useState('deepseek/deepseek-flash');
  const [apiKey, setApiKey] = useState('');
  const [provider, setProvider] = useState('deepseek');
  const [testCommand, setTestCommand] = useState('["node", "--test"]');
  const [planText, setPlanText] = useState('');
  const [runId, setRunId] = useState('');
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [busy, setBusy] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [historyRevision, setHistoryRevision] = useState(0);
  const [history, setHistory] = useState<{ projectId: string; documentToken: string; value: ProjectHistory } | null>(null);
  const [historyError, setHistoryError] = useState('');
  const [historyLoading, setHistoryLoading] = useState(false);
  const currentDocument = useRef(currentProject);
  currentDocument.current = currentProject;
  const archived = history?.projectId === currentProject.projectId && history.documentToken === currentProject.documentToken ? history.value : null;
  const workflow = snapshot.workflows.find(w => w.id === workflowId);
  const run = workflow?.runs.find(r => r.id === runId);
  const planSource = workflow?.plan ?? workflow?.draftPlan;

  useEffect(() => {
    if (!native) return;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try { const data = await aiSnapshot(); if (!stopped) setSnapshot(data); }
      catch (cause) { if (!stopped) setError(String(cause)); }
      if (!stopped) timer = setTimeout(() => void poll(), 700);
    }
    void poll();
    return () => { stopped = true; clearTimeout(timer); };
  }, []);
  useEffect(() => { setPlanText(planSource ? JSON.stringify(planSource, null, 2) : ''); }, [workflowId, JSON.stringify(planSource)]);
  useEffect(() => { setNotice(''); }, [currentProject.documentToken]);
  useEffect(() => {
    if (!native || !historyOpen || !currentProject.projectId || !currentProject.documentToken) return;
    let stopped = false;
    setHistoryLoading(true); setHistoryError('');
    projectArchivedHistory(currentProject.projectId, currentProject.documentToken)
      .then(value => { if (!stopped) setHistory({ projectId: currentProject.projectId, documentToken: currentProject.documentToken, value }); })
      .catch((cause: unknown) => { if (!stopped) setHistoryError(String(cause)); })
      .finally(() => { if (!stopped) setHistoryLoading(false); });
    return () => { stopped = true; };
  }, [currentProject.projectId, currentProject.documentToken, currentProject.archivedWorkflowCount, historyOpen, historyRevision]);

  async function action(operation: () => Promise<void>) {
    if (busy) return;
    setBusy(true); setError(''); setNotice('');
    try { await operation(); } catch (cause) { setError(String(cause)); } finally { setBusy(false); }
  }
  async function command(command: object) { setSnapshot(await aiCommand(command)); }
  async function chooseProject() {
    const directory = await open({ directory: true, multiple: false, title: '选择代码项目' });
    if (!directory) return;
    const files = await aiScan(directory);
    setProject(directory); setPaths(files); setSelected([]);
  }
  async function create() {
    const argv: unknown = JSON.parse(testCommand);
    if (!Array.isArray(argv) || argv.some(v => typeof v !== 'string')) throw new Error('测试命令须为字符串数组；不运行测试可填 []');
    const data = await aiCommand({ type: 'create', project, paths: selected, goal, model, test_command: argv });
    setSnapshot(data); setWorkflowId(data.workflows.at(-1)!.id); setRunId('');
  }
  async function testResult(result: Run) {
    if (!workflow) return;
    if (!await ask(`将在独立目录执行生成的代码：\n${workflow.testCommand.join(' ')}\n这使用本机权限，不是安全沙箱。继续运行测试？`, { title: '运行代码测试', kind: 'warning', okLabel: '运行测试', cancelLabel: '取消' })) return;
    await command({ type: 'test', id: workflow.id, run_id: result.id });
  }
  async function exportResult(result: Run) {
    const directory = await open({ directory: true, title: '导出已审阅代码快照' });
    if (directory && workflow) setNotice(`已导出：${await aiExport(workflow.id, result.id, directory)}`);
  }
  async function archiveWorkflow() {
    if (!workflow) return;
    const { projectId, documentToken } = currentProject;
    const next = await archiveProjectWorkflow(projectId, documentToken, workflow.id);
    if (currentDocument.current.projectId !== projectId || currentDocument.current.documentToken !== documentToken) return;
    onProjectChange(next);
    setHistoryRevision(value => value + 1);
    setNotice('已将此流程的当前记录加入项目。保存 .mio 项目文件后可随文件重新打开查看。');
  }

  return <main className="collaboration-workspace">
    <section className="collaboration-main" aria-label="代码协作">
      <div className="collaboration-heading"><div><p className="eyebrow">CODE · AGENTS · REVIEW</p><h1>一起，把代码做好</h1><p>选择源码，规划分工，审阅每一次交接。</p></div><span className="engine-badge">内置 Pi Agent</span></div>
      {!native && <p className="banner">浏览器仅预览；请在 Ravomi 桌面应用中运行代码协作。</p>}
      {error && <div role="alert" className="banner error">{error}</div>}
      {notice && <p role="status" className="banner">{notice}</p>}
      <details aria-label="已保存协作历史" onToggle={event => setHistoryOpen(event.currentTarget.open)}>
        <summary>已保存协作历史（{currentProject.archivedWorkflowCount}）</summary>
        <p className="muted">当前项目：{currentProject.title}。这里保留固定的输入、计划、运行与审阅记录；运行状态为保存此记录时的状态，仅供查看。</p>
        {!native ? <p className="muted">请在桌面应用中打开项目查看历史。</p> : historyLoading ? <p role="status">正在读取项目历史…</p> : historyError ? <div className="banner error" role="alert">{historyError}<button onClick={() => setHistoryRevision(value => value + 1)}>重新读取历史</button></div> : archived && (archived.workflows.length === 0 ? <p className="muted">尚未向此项目保存协作历史。选择一个本机流程后，可以保存它的当前记录。</p> : archived.workflows.map(saved => <details key={saved.id}>
          <summary>{saved.goal} · {saved.runs.length} 次运行</summary>
          <p className="muted">{saved.model} · {saved.files.length} 个固定输入 · 只读历史</p>
          <pre>{JSON.stringify(saved, null, 2)}</pre>
        </details>))}
      </details>
      <details className="model-settings"><summary>模型连接设置</summary><div className="ai-config"><label>提供方<select aria-label="模型提供方" value={provider} onChange={e => setProvider(e.target.value)}><option value="deepseek">DeepSeek</option><option value="openai">OpenAI</option><option value="anthropic">Anthropic</option></select></label><label>API Key<input aria-label="模型 API Key" type="password" autoComplete="off" value={apiKey} onChange={e => setApiKey(e.target.value)} /></label></div><button disabled={!native || busy || !apiKey.trim()} onClick={() => void action(async () => { await aiConfigure(provider, apiKey); setApiKey(''); setNotice('已更新当前会话的模型凭据；尚未验证网络调用。'); })}>应用到本次会话</button><p className="muted">密钥只保留在当前进程内，不进入项目或运行记录。关闭应用后清除；未填写时使用 API Key 环境变量或已有 Pi API-key 配置。</p></details>
      {snapshot.storageError && <div role="alert" className="banner error">运行记录未保存：{snapshot.storageError}<button onClick={() => void action(() => command({ type: 'flush' }))}>恢复保存</button></div>}
      <div className="workflow-switcher"><label>协作流程<select aria-label="协作流程" value={workflowId} onChange={e => { setWorkflowId(e.target.value); setRunId(''); }}><option value="">新建代码流程</option>{snapshot.workflows.map(w => <option key={w.id} value={w.id}>{w.goal.slice(0, 50)}</option>)}</select></label><button onClick={() => { setWorkflowId(''); setRunId(''); }}>新建流程</button></div>
      {!workflow ? <div className="collaboration-setup">
        <label htmlFor="ai-goal">这次要完成什么？</label><textarea id="ai-goal" value={goal} onChange={e => setGoal(e.target.value)} placeholder="例如：实现日期解析模块，补齐边界测试，再由检查 Agent 审阅。" rows={3} />
        <div className="ai-config"><label>模型<input aria-label="协作模型" value={model} onChange={e => setModel(e.target.value)} placeholder="provider/model" /></label><label>测试命令（参数数组）<input aria-label="测试命令" value={testCommand} onChange={e => setTestCommand(e.target.value)} /></label></div>
        <p className="muted">引擎内置 DeepSeek、OpenAI、Anthropic 适配；使用相应 API Key 环境变量或已有 Pi API-key 登录。OAuth 与自定义端点尚未接入。测试需另行点击授权，不自动执行。</p>
        <button disabled={!native || busy} onClick={() => void action(chooseProject)}><Icon name="folder" />选择代码目录</button><p className="source-path">{project || '尚未选择目录'}</p>
        <fieldset className="source-picker"><legend>本次发送的文件（{selected.length} / 64）</legend>{paths.length === 0 ? <p className="muted">选择目录后勾选需要的 UTF-8 源码；不会默认发送整个项目。</p> : paths.map(path => <label key={path}><input type="checkbox" checked={selected.includes(path)} disabled={!selected.includes(path) && selected.length >= 64} onChange={e => setSelected(e.target.checked ? [...selected, path] : selected.filter(p => p !== path))} /><span>{path}</span></label>)}</fieldset>
        <p className="muted">点击后，将所选文件快照与目标发送到模型提供方，生成待审阅计划。原项目保持不变；总输入上限 512 KiB。</p>
        <button className="primary" disabled={!native || busy || !goal.trim() || !selected.length || !model.trim()} onClick={() => void action(create)}>生成协作计划</button>
      </div> : <>
        <div className="workflow-summary"><h2>{workflow.goal}</h2><p className="muted">{workflow.model} · {workflow.files.length} 个固定输入 · {workflow.runs.length}/{workflow.maxCalls} 次运行 · 最多 2 路并行</p><p className="source-path">{workflow.project}</p></div>
        <div className="workflow-controls"><button disabled={!native || busy || projectBusy || !currentProject.projectId || !currentProject.documentToken} onClick={() => void action(() => runProjectAction(archiveWorkflow))}>保存到当前项目</button><span className="muted">仅保存此流程到「{currentProject.title}」；包含已选源码和运行记录。再次保存会更新此流程的归档，其他已保存流程保留。</span></div>
        <details><summary>查看实际输入与执行范围</summary><p>只发送已选文本与显式依赖成果；不读取其他项目文件，不自动安装依赖。每次运行最长 {workflow.timeoutSeconds} 秒。</p>{workflow.files.map(file => <details key={file.path}><summary>{file.path}</summary><pre>{file.content}</pre></details>)}</details>
        {workflow.runs.filter(r => r.taskId === '$plan').map(r => <button className="planning-state" key={r.id} onClick={() => setRunId(r.id)}>规划 Agent · {labels[r.status]}{r.error ? ` · ${r.error}` : ''}</button>)}
        {planSource && !workflow.runs.some(r => !r.taskId.startsWith('$')) && <details open className="plan-editor"><summary>审阅与编辑任务计划</summary><p className="muted">可以调整职责、指令、验收条件、dependencies 与 parentAgent；保存计划不会执行任务。</p><textarea aria-label="任务计划 JSON" rows={12} value={planText} onChange={e => setPlanText(e.target.value)} spellCheck={false} /><button disabled={busy} onClick={() => void action(() => command({ type: 'set_plan', id: workflow.id, plan: JSON.parse(planText) }))}>保存计划</button></details>}
        {workflow.plan && <>
          <div className="workflow-controls"><button className="primary" disabled={busy || !workflow.paused} onClick={() => void action(() => command({ type: 'start', id: workflow.id }))}>运行 / 恢复派发</button><button disabled={busy || workflow.paused} onClick={() => void action(() => command({ type: 'pause', id: workflow.id }))}>暂停派发</button><button disabled={busy} onClick={() => void action(() => command({ type: 'stop', id: workflow.id, task_id: null }))}>停止整个流程</button><span className="muted">{workflow.paused ? '派发已暂停；运行中的任务可能仍在继续' : '按依赖派发任务'}</span></div>
          <div className="agent-board">{workflow.plan.tasks.map(task => {
            const latest = last(workflow, task.id);
            const waiting = task.dependencies.filter(d => last(workflow, d)?.status !== 'succeeded');
            return <article className="agent-task" key={task.id} data-status={latest?.status ?? 'waiting'}><div className="agent-task-title"><h3>{task.title}</h3><span>{latest ? labels[latest.status] : waiting.length ? '等待依赖' : '待派发'}</span></div><p className="muted">{task.role}{task.parentAgent ? ` · 子 Agent，父任务：${task.parentAgent}` : ''}</p><p>{task.instruction}</p><p className="acceptance">验收：{task.acceptance}</p><p className="muted">输入来自：{task.dependencies.join('、') || '选定源码'}{waiting.length ? `；等待 ${waiting.join('、')}` : ''}</p>{latest && <button onClick={() => setRunId(latest.id)}>查看运行与成果</button>}{latest?.status === 'succeeded' && <span className="review-state">{latest.review === 'approved' ? '人工审阅通过' : latest.review === 'changes_requested' ? '要求修改' : latest.review === 'rejected' ? '已拒绝' : '尚未人工审阅'}</span>}</article>;
          })}</div>
        </>}
        <details className="run-history"><summary>运行历史与测试（{workflow.runs.length}）</summary>{workflow.runs.map(r => <button key={r.id} onClick={() => setRunId(r.id)}>{r.taskId === '$test' ? '代码测试' : r.taskId} · {labels[r.status]} · {new Date(r.createdAt * 1000).toLocaleTimeString()}</button>)}</details>
      </>}
    </section>
    <aside className="collaboration-detail" aria-label="协作详情"><h2>运行与审阅</h2>{run && workflow ? <>
      <p>{run.taskId} · {labels[run.status]}</p><p className="muted">工件版本：{run.id}</p>
      {run.error && <p role="alert" className="error">{run.error}</p>}
      <div className="run-actions">{['failed', 'cancelled'].includes(run.status) && <button disabled={busy || last(workflow, run.taskId)?.id !== run.id} onClick={() => void action(() => command({ type: 'retry', id: workflow.id, run_id: run.id }))}>用固定输入重试</button>}{run.status === 'unknown' && <button onClick={() => void action(async () => { if (await ask('已核对旧运行的进程结束，且不再产生副作用？', { title: '确认中断', kind: 'warning' })) await command({ type: 'resolve', id: workflow.id, run_id: run.id }); })}>核对后标记中断</button>}{['running', 'queued'].includes(run.status) && <button disabled={busy} onClick={() => void action(() => command({ type: 'stop', id: workflow.id, task_id: run.taskId }))}>停止此分支</button>}</div>
      <details><summary>实际输入快照</summary><pre>{run.prompt}</pre></details>
      <p className="muted">交接来源：{run.sources.join('、') || '初始源码快照'}</p>
      {run.proposal ? <><h3>成果说明</h3><p className="result-summary">{run.proposal.summary}</p>{run.proposal.files.map(file => <details key={file.path}><summary>{file.path} · 完整新版本</summary><h4>原版本</h4><pre>{run.inputFiles.find(f => f.path === file.path)?.content ?? '新增文件'}</pre><h4>新版本</h4><pre>{file.content}</pre></details>)}</> : <pre className="stream-output">{run.output || '等待输出…'}</pre>}
      {run.status === 'succeeded' && run.proposal && <div className="review-actions"><button disabled={busy || !workflow.testCommand.length} onClick={() => void action(() => testResult(run))}>在独立目录运行测试</button><button className="primary" disabled={busy} onClick={() => void action(() => command({ type: 'review', id: workflow.id, run_id: run.id, decision: 'approved' }))}>审阅通过</button><button disabled={busy} onClick={() => void action(() => command({ type: 'review', id: workflow.id, run_id: run.id, decision: 'changes_requested' }))}>要求修改</button><button disabled={busy || run.review !== 'approved'} onClick={() => void action(() => exportResult(run))}>导出代码快照</button><p className="muted">导出到新目录，不覆盖原项目；测试结果与此工件版本关联。需要修改时，以新目标创建流程，保留原记录。</p></div>}
      <details><summary>提供方上报用量</summary><pre>{run.usage ? JSON.stringify(run.usage, null, 2) : '未上报 / 不适用'}</pre></details>
    </> : <p className="muted">选择一个 Agent 的运行，查看输入、交接、代码和测试记录。生成成功与人工审阅分别记录。</p>}</aside>
  </main>;
}
