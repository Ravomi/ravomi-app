import fs from 'node:fs/promises';

const [port, script] = process.argv.slice(2);
const pages = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = pages.find(p => p.type === 'page' && p.url.startsWith('tauri://'));
if (!page) throw new Error('Project Graph page not found');
const socket = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject; });
const expression = await fs.readFile(script, 'utf8');
socket.send(JSON.stringify({ id: 1, method: 'Runtime.evaluate', params: { expression, awaitPromise: true, returnByValue: true } }));
socket.onmessage = event => {
    const response = JSON.parse(event.data);
    if (response.id !== 1) return;
    console.log(JSON.stringify(response, null, 2));
    if (response.error || response.result?.exceptionDetails) process.exitCode = 1;
    socket.close();
};
setTimeout(() => { console.error('CDP timeout'); process.exit(1); }, 55000).unref();
