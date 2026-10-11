import { useLayoutEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';

interface EditState {
  session: number; text: string; fontSize: number; contentWidth: number;
  autoSize: boolean; light: boolean; background?: string;
}
declare global {
  interface Window {
    ipc?: { postMessage(message: string): void };
    setCanvasEdit?: (state: EditState) => void;
  }
}

function post(message: object) { window.ipc?.postMessage(JSON.stringify(message)); }

function CanvasEditor() {
  const [state, setState] = useState<EditState | null>(null);
  const [draft, setDraft] = useState('');
  const input = useRef<HTMLTextAreaElement>(null);
  const measure = useRef<HTMLDivElement>(null);
  const composing = useRef(false);
  const session = useRef<number | null>(null);
  const finished = useRef(false);
  useLayoutEffect(() => {
    window.setCanvasEdit = (next) => {
      if (session.current !== next.session) {
        session.current = next.session;
        composing.current = false;
        finished.current = false;
        setDraft(next.text);
      }
      setState(next);
    };
    post({ type: 'ready' });
    return () => { delete window.setCanvasEdit; };
  }, []);
  useLayoutEffect(() => {
    if (!state) return;
    input.current?.focus();
    input.current?.setSelectionRange(input.current.value.length, input.current.value.length);
  }, [state?.session]);

  function dimensions(text: string) {
    if (!measure.current) return { width: 1, height: 1 };
    measure.current.textContent = `${text || ' '}\u200b`;
    return { width: Math.ceil(measure.current.getBoundingClientRect().width), height: Math.ceil(measure.current.getBoundingClientRect().height) };
  }
  function update(text: string, isComposing: boolean) {
    if (!state || finished.current) return;
    setDraft(text);
    post({ type: 'draft', session: state.session, text, composing: isComposing, ...dimensions(text) });
  }
  function finish() {
    if (!state || composing.current || finished.current) return;
    finished.current = true;
    const text = input.current?.value ?? draft;
    post({ type: 'commit', session: state.session, text, ...dimensions(text) });
  }
  if (!state) return null;
  return <div data-theme={state.light ? 'light' : 'dark'} style={{ background: state.background ?? (state.light ? '#ffffff' : '#292a31'), height: '100%' }}>
    <div ref={measure} className="measure" aria-hidden="true" style={{ fontSize: state.fontSize, width: state.autoSize ? 'max-content' : state.contentWidth, whiteSpace: state.autoSize ? 'pre' : 'pre-wrap' }} />
    <textarea ref={input} aria-label="节点文字" value={draft} spellCheck={false}
      title="Enter 完成 · Shift+Enter 换行 · Esc 结束编辑"
      style={{ fontSize: state.fontSize, width: '100%', height: '100vh', whiteSpace: state.autoSize ? 'pre' : 'pre-wrap' }}
      onChange={(event) => update(event.target.value, composing.current)}
      onCompositionStart={() => { composing.current = true; update(input.current?.value ?? draft, true); }}
      onCompositionEnd={(event) => { composing.current = false; update(event.currentTarget.value, false); }}
      onKeyDown={(event) => {
        if (composing.current || event.nativeEvent.isComposing || event.keyCode === 229) return;
        if ((event.key === 'Enter' && !event.shiftKey) || event.key === 'Escape') { event.preventDefault(); finish(); }
        if (event.key === 'Tab') { event.preventDefault(); finish(); }
      }}
      onBlur={() => { if (document.hasFocus() && !composing.current) finish(); }} />
  </div>;
}

createRoot(document.getElementById('root')!).render(<CanvasEditor />);
