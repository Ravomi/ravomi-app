// Node >=22: built-in HTTP, subprocess and WebSocket APIs; no automation dependency.
import {createServer} from 'node:http';
import {readFile,writeFile,mkdtemp,rm,mkdir} from 'node:fs/promises';
import {spawn} from 'node:child_process';
import {tmpdir} from 'node:os';
import {join,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
const root=dirname(fileURLToPath(import.meta.url));
const scene=process.argv[2], output=process.argv[3];
const width=Number(process.env.BENCHMARK_WIDTH)||1280,height=Number(process.env.BENCHMARK_HEIGHT)||800;
if(!Number.isInteger(width)||!Number.isInteger(height)||width<1||height<1)throw new Error('Invalid benchmark dimensions');
if(!scene||!output)throw new Error('Usage: node benchmark/canvas.mjs scene.json results.json');
const server=createServer(async(req,res)=>{
  try{res.setHeader('Content-Type',req.url==='/scene.json'?'application/json':'text/html');res.end(await readFile(req.url==='/scene.json'?scene:join(root,'canvas.html')));}
  catch(e){res.statusCode=500;res.end(String(e));}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const profile=await mkdtemp(join(tmpdir(),'prg-canvas-chrome-'));
const chrome=spawn(process.env.CHROME||'/usr/bin/google-chrome',[
  `--user-data-dir=${profile}`,'--remote-debugging-port=0','--no-first-run','--no-default-browser-check',
  '--disable-frame-rate-limit','--disable-gpu-vsync','--disable-background-timer-throttling','--disable-renderer-backgrounding',
  '--force-device-scale-factor=1',`--app=http://127.0.0.1:${server.address().port}?width=${width}&height=${height}`,`--window-size=${width},${height}`
],{stdio:'ignore'});
let ws;
try{
  let port;
  for(let i=0;i<100;i++){try{port=Number((await readFile(join(profile,'DevToolsActivePort'),'utf8')).split('\n')[0]);break;}catch{await new Promise(r=>setTimeout(r,100));}}
  if(!port)throw new Error('Chrome debugging endpoint did not start');
  const targets=await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
  const target=targets.find(t=>t.type==='page');if(!target)throw new Error('Missing benchmark tab');
  ws=new WebSocket(target.webSocketDebuggerUrl);await new Promise((resolve,reject)=>{ws.onopen=resolve;ws.onerror=reject;});
  let id=0;const pending=new Map();
  ws.onmessage=event=>{const msg=JSON.parse(event.data);if(pending.has(msg.id)){const {resolve,reject}=pending.get(msg.id);pending.delete(msg.id);msg.error?reject(new Error(JSON.stringify(msg.error))):resolve(msg.result);}};
  const call=(method,params={})=>new Promise((resolve,reject)=>{const n=++id;pending.set(n,{resolve,reject});ws.send(JSON.stringify({id:n,method,params}));});
  await call('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false});
  for(let i=0;i<100;i++){const state=await call('Runtime.evaluate',{expression:'window.ready',returnByValue:true});if(state.result.value)break;await new Promise(r=>setTimeout(r,100));}
  const measured=await call('Runtime.evaluate',{expression:'window.runBenchmark()',awaitPromise:true,returnByValue:true});
  if(measured.exceptionDetails)throw new Error(JSON.stringify(measured.exceptionDetails));
  const version=await call('Browser.getVersion');
  measured.result.value.browser=version.product;
  const gpu=await call('Runtime.evaluate',{expression:`(()=>{const gl=document.createElement('canvas').getContext('webgl');if(!gl)return 'unavailable';const ext=gl.getExtension('WEBGL_debug_renderer_info');return ext?gl.getParameter(ext.UNMASKED_RENDERER_WEBGL):gl.getParameter(gl.RENDERER);})()`,returnByValue:true});
  measured.result.value.webgl_renderer=gpu.result.value;
  await mkdir(dirname(output),{recursive:true});await writeFile(output,JSON.stringify(measured.result.value,null,2));
  console.log(JSON.stringify(measured.result.value,null,2));
}finally{ws?.close();chrome.kill();server.close();await new Promise(r=>setTimeout(r,500));await rm(profile,{recursive:true,force:true});}
