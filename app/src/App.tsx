import { useEffect, useRef, useState } from 'react';
import { ask, open, save } from '@tauri-apps/plugin-dialog';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { execute, getSnapshot, native, type Command, type Snapshot } from './bridge';
import { Icon } from './Icon';
import { Collaboration } from './Collaboration';
import { aiHasActive, aiShutdown } from './collaboration';
import avatar from '../../assets/branding/ravomi-avatar.png';
import appIcon from '../../assets/branding/app-icons/128x128.png';

const empty: Snapshot = {
  projectId: '', documentToken: '', archivedWorkflowCount: 0,
  title: '未命名画布', path: null, dirty: false, canUndo: false,
  canRedo: false, nodes: [], edgeCount: 0,
};

export function App() {
  const [document, setDocument] = useState<Snapshot>(empty);
  const [selected, setSelected] = useState<string | null>(null);
  const [draft, setDraft] = useState('');
  const [light, setLight] = useState(true);
  const [scale, setScale] = useState('100');
  const [busy, setBusy] = useState(native);
  const [ready, setReady] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [view, setView] = useState<'canvas' | 'collaboration'>('canvas');
  const busyRef = useRef(native);
  const node = document.nodes.find((item) => item.id === selected);
  const hasDraft = Boolean(node?.editable && draft !== node.text);
  const unsaved = document.dirty || hasDraft;
  const unsavedRef = useRef(unsaved);
  unsavedRef.current = unsaved;

  useEffect(() => {
    if (!native) return;
    let active = true;
    getSnapshot().then((snapshot) => {
      if (active) { setDocument(snapshot); setReady(true); }
    }).catch((cause: unknown) => {
      if (active) setError(String(cause));
    }).finally(() => {
      if (active) { setBusy(false); busyRef.current = false; }
    });
    return () => { active = false; };
  }, []);

  useEffect(() => {
    if (!native) return;
    let active = true;
    let unlisten: (() => void) | undefined;
    let asking = false;
    getCurrentWindow().onCloseRequested(async (event) => {
      // Block closing during file operations and while a confirmation is open.
      if (busyRef.current || asking) { event.preventDefault(); return; }
      event.preventDefault();
      asking = true;
      try {
        const active = await aiHasActive();
        if ((unsavedRef.current || active) && !await ask([
          unsavedRef.current ? '尚有未保存的画布修改，关闭将放弃这些修改。' : '',
          active ? '代码协作仍有活动运行，关闭前会请求停止并保留运行记录。' : '',
        ].filter(Boolean).join('\n'), {
          title: '关闭 Ravomi', kind: 'warning', okLabel: '停止并关闭', cancelLabel: '继续工作',
        })) return;
        await aiShutdown();
        await getCurrentWindow().destroy();
      } catch (cause) { setError(String(cause)); }
      finally { asking = false; }
    }).then((stop) => { if (active) unlisten = stop; else stop(); })
      .catch((cause: unknown) => { if (active) setError(`无法监听窗口关闭：${String(cause)}`); });
    return () => { active = false; unlisten?.(); };
  }, []);

  async function run(operation: () => Promise<void>) {
    if (busyRef.current || !ready) return;
    busyRef.current = true;
    setBusy(true); setError(''); setNotice('');
    try { await operation(); }
    catch (cause) { setError(String(cause)); }
    finally { busyRef.current = false; setBusy(false); }
  }

  async function command(value: Command) {
    const snapshot = await execute(value);
    setDocument(snapshot);
    return snapshot;
  }

  async function discard(message: string) {
    return ask(message, { title: '保留更改？', kind: 'warning', okLabel: '放弃更改', cancelLabel: '继续编辑' });
  }

  async function replaceDocument(kind: 'new' | 'open') {
    if (unsaved && !await discard('当前画布尚未保存。继续将放弃更改。')) return;
    if (kind === 'new') await command({ type: 'new', discard: true });
    else {
      const path = await open({ multiple: false, filters: [{ name: 'Ravomi 项目与兼容画布', extensions: ['mio', 'prg'] }] });
      if (!path) return;
      await command({ type: 'open', path, discard: true });
    }
    setSelected(null); setDraft('');
  }

  async function applyDraft() {
    if (node && hasDraft) {
      await command({ type: 'edit_text', id: node.id, text: draft });
    }
  }

  async function saveDocument(copy: boolean) {
    await applyDraft();
    const path = !copy && document.path ? document.path : await save({
      defaultPath: document.path?.replace(/\.prg$/i, '.mio') ?? '未命名项目.mio',
      filters: [{ name: 'Ravomi 项目 (.mio)', extensions: ['mio'] }, { name: '兼容画布 (.prg)', extensions: ['prg'] }],
    });
    if (!path) return;
    await command({ type: 'save', path });
    setNotice('画布已保存');
  }

  async function history(type: 'undo' | 'redo') {
    if (hasDraft && !await discard('文字尚未应用。继续将放弃这段修改。')) return;
    const snapshot = await command({ type });
    setDraft(snapshot.nodes.find((item) => item.id === selected)?.text ?? '');
  }

  async function select(id: string) {
    if (id === selected) return;
    if (hasDraft && !await discard('文字尚未应用。切换对象将放弃这段修改。')) return;
    setSelected(id);
    setDraft(document.nodes.find((item) => item.id === id)?.text ?? '');
  }

  async function addText() {
    if (hasDraft) await applyDraft();
    const offset = document.nodes.length * 24;
    const snapshot = await command({ type: 'add_text', x: offset, y: offset });
    const added = snapshot.nodes.at(-1);
    if (added) { setSelected(added.id); setDraft(added.text); }
  }

  const disabled = busy || !ready;
  const macWindow = native && /Mac/.test(navigator.platform);
  return <div className="app" data-macos={macWindow || undefined} data-theme={light ? 'light' : 'dark'} style={{ zoom: Number(scale) / 100, minHeight: `${10000 / Number(scale)}dvh` }}>
    {macWindow && <div className="macos-titlebar" style={{ height: 3600 / Number(scale), paddingLeft: 8800 / Number(scale) }} aria-hidden="true"><div data-tauri-drag-region /></div>}
    <aside className="navigation" aria-label="主导航">
      <div className="navigation-brand"><img src={appIcon} alt="" /><div>Ravomi<small>人和 Agent，共绘工作</small></div></div>
      <p className="nav-label">工作空间</p>
      <button className={view === 'canvas' ? 'nav-current' : ''} aria-pressed={view === 'canvas'} onClick={() => setView('canvas')}><Icon name="grid" />我的画布</button>
      <button className={view === 'collaboration' ? 'nav-current' : ''} aria-pressed={view === 'collaboration'} onClick={() => setView('collaboration')}><Icon name="text" />代码协作</button>
      <button disabled={disabled} onClick={() => void run(() => replaceDocument('new'))}><Icon name="plus" />新建</button>
      <button disabled={disabled} onClick={() => void run(() => replaceDocument('open'))}><Icon name="folder" />打开文件</button>
      <button disabled={disabled} onClick={() => void run(() => saveDocument(false))}><Icon name="save" />保存画布</button>
      <div className="navigation-note"><Icon name="info" /><p>从一个想法出发。<br />让工作，在这里展开。</p><small>Pi 代码协作 · MVP</small></div>
      <div className="navigation-footer">Ravomi <span>by Arcraven</span></div>
    </aside>
    <div className="app-content">
    <header className="topbar">
      <div className="brand" aria-label="Ravomi"><img src={appIcon} alt="" />Ravomi</div>
      <div className="document-tab" title={document.path ?? document.title}><Icon name="file" /><span>{document.title}{unsaved ? ' · 未保存' : ''}</span></div>
      <div className="window-actions">
        <button aria-label={view === 'canvas' ? '打开代码协作' : '返回画布'} onClick={() => setView(view === 'canvas' ? 'collaboration' : 'canvas')}><Icon name="text" /><span>{view === 'canvas' ? '协作' : '画布'}</span></button>
        <button onClick={() => setLight(!light)} aria-label={light ? '切换深色主题' : '切换浅色主题'}><Icon name="moon" /><span>{light ? '深色' : '浅色'}</span></button>
        <label className="scale"><span className="sr-only">界面缩放</span><select value={scale} onChange={(event) => setScale(event.target.value)}>
          <option value="100">100%</option><option value="150">150%</option><option value="200">200%</option>
        </select></label>
        <details className="menu"><summary aria-label="工作区菜单">工作区</summary><div className="menu-items">
          <button disabled={disabled} onClick={() => void run(() => replaceDocument('new'))}>新建画布</button>
          <button disabled={disabled} onClick={() => void run(() => replaceDocument('open'))}>打开画布…</button>
          <button disabled={disabled} onClick={() => void run(() => saveDocument(false))}>保存</button>
          <button disabled={disabled} onClick={() => void run(() => saveDocument(true))}>另存为…</button>
        </div></details>
      </div>
    </header>

    {!native && <p className="banner">当前为浏览器预览。文档编辑暂在桌面应用中使用，浏览器版仍在开发。</p>}
    {error && <div className="banner error" role="alert"><span>{error}</span><button onClick={() => setError('')}>关闭提示</button></div>}
    {view === 'collaboration' ? <Collaboration currentProject={document} projectBusy={busy} runProjectAction={run} onProjectChange={next => setDocument(current => current.projectId === next.projectId && current.documentToken === next.documentToken ? next : current)} /> : <main>
      <section className="workspace" id="workspace" aria-label="画布对象" tabIndex={-1}>
        <div className="workspace-heading"><div><p className="eyebrow">YOUR SPACE, YOUR IDEAS</p><h1>让灵感安静生长</h1><p className="hero-description">记录、连接，逐步展开你的工作。</p>
          <button className="primary" disabled={disabled} onClick={() => void run(addText)}><Icon name="plus" />新建文字</button></div>
          <img className="hero-avatar" src={avatar} alt="Ravomi 团子猫" />
        </div>
        <div className="object-toolbar"><div><strong>画布内容</strong><span className="count">{document.nodes.length}</span></div>
          <div className="history-actions"><button disabled={disabled || !document.canUndo} onClick={() => void run(() => history('undo'))}><Icon name="undo" />撤销</button>
            <button disabled={disabled || !document.canRedo} onClick={() => void run(() => history('redo'))}><Icon name="redo" />重做</button></div>
        </div>
        {document.nodes.length === 0 ? <div className="empty-state"><div className="empty-mark"><Icon name="grid" /></div><h2>这里，留给你的下一个想法</h2><p>添加第一段文字，或从已有画布继续。</p><div className="empty-actions">
          <button disabled={disabled} onClick={() => void run(() => replaceDocument('open'))}><Icon name="folder" />打开画布</button>
        </div></div> : <div className="object-grid">{document.nodes.map((item) => <button
          className="object-card" key={item.id} aria-pressed={item.id === selected} disabled={disabled}
          onClick={() => void run(() => select(item.id))} title={item.text || '空白对象'}>
          <span className="object-kind"><Icon name={item.kind === 'TextNode' ? 'text' : 'grid'} />{item.kind === 'TextNode' ? '文字' : item.kind === 'Section' ? '分组' : '其他对象'}</span>
          <span className="object-text">{item.text || '空白对象'}</span>
          <span className="muted">{item.editable ? '选择以编辑' : '仅查看'}</span>
        </button>)}</div>}
        <p className="view-note">当前显示对象列表；空间画布、连线和图片预览正在迁移。</p>
      </section>

      <aside className="inspector" aria-label="对象属性"><p className="eyebrow">DETAILS</p><h2>对象属性</h2>{node ? <>
        <label htmlFor="node-text">文字内容</label>
        <textarea id="node-text" value={draft} onChange={(event) => setDraft(event.target.value)} disabled={disabled || !node.editable} rows={8} />
        <p className="muted">{node.editable ? '应用后可以撤销。保存会一并应用文字修改。' : '此对象已锁定，或暂不支持在这里编辑。'}</p>
        <button className="primary" disabled={disabled || !hasDraft} onClick={() => void run(applyDraft)}>应用修改</button>
      </> : <div className="inspector-empty"><Icon name="text" /><p>细节，按需展开</p><p className="muted">选择一个对象，<br />在这里查看和编辑它的内容。</p></div>}</aside>
    </main>}
    <footer><span role="status"><span className="status-dot" />{busy ? '正在处理…' : notice || (unsaved ? '更改尚未保存' : '就绪')}</span><span>{document.nodes.length} 个对象 · {document.edgeCount} 条连线</span></footer>
    </div>
  </div>;
}
