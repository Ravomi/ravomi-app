(async () => {
    const p = window.__grapluBenchmarkProject;
    if (!p) throw new Error('Inspect the active project first');
    const camera = p.camera;
    const Vector = camera.location.constructor;
    const originalCamera = { location: camera.location.clone(), currentScale: camera.currentScale, targetScale: camera.targetScale };
    const oldRender = p.renderer.tick_;
    const entities = p.stageManager.getEntities();
    const moving = entities.filter(e => !('children' in e) && !e.locked);
    const displacement = new Vector(0, 0);
    const results = [];
    const summarize = samples => {
        if (!samples.length) return null;
        samples.sort((a,b)=>a-b);
        return { samples: samples.length, mean_ms:samples.reduce((a,b)=>a+b,0)/samples.length, p95_ms:samples[Math.floor(samples.length*.95)],p99_ms:samples[Math.floor(samples.length*.99)] };
    };
    camera.stopImmediately();
    camera.reset();
    p.controller.resetCountdownTimer();
    await new Promise(resolve=>setTimeout(resolve,4000));
    const home = camera.location.clone();
    const scale = camera.currentScale;
    let renderTimes = [];
    let renderStarts = [];
    p.renderer.tick_ = function(...args) {const start=performance.now();renderStarts.push(start);try{return oldRender.apply(this,args);}finally{renderTimes.push(performance.now()-start);}};
    try {
        for (const phase of ['idle','pan','zoom','drag_all']) {
            renderTimes=[];
            renderStarts=[];
            const frames=[];
            let unfocusedFrames=0;
            let last;
            const start=performance.now();
            await new Promise(resolve=>{
                function frame(now) {
                    p.controller.resetCountdownTimer();
                    if(!document.hasFocus())unfocusedFrames++;
                    const elapsed=(now-start)/1000;
                    const dt=last===undefined?0:(now-last)/1000;
                    if(last!==undefined)frames.push(now-last);
                    last=now;
                    if(elapsed>=4){resolve();return;}
                    if(phase==='pan') {
                        camera.location=new Vector(home.x+Math.sin(elapsed*2)*700,home.y-Math.cos(elapsed*2)*400);
                        camera.targetLocationByScale=camera.location.clone();
                    } else if(phase==='zoom') {
                        camera.currentScale=scale/(.55+.45*Math.abs(Math.cos(elapsed*2)));
                        camera.targetScale=camera.currentScale;
                    } else if(phase==='drag_all') {
                        const delta=new Vector(Math.cos(elapsed*2)*dt*160,-Math.sin(elapsed*2)*dt*160);
                        for(const node of moving)node.move(delta);
                        displacement.x+=delta.x;displacement.y+=delta.y;
                    }
                    requestAnimationFrame(frame);
                }
                requestAnimationFrame(frame);
            });
            const intervals=summarize(frames);
            if(renderTimes.length<2)throw new Error('Renderer did not draw during '+phase);
            const rendered=summarize(renderStarts.slice(1).map((t,i)=>t-renderStarts[i]));
            results.push({phase,unfocused_frames:unfocusedFrames,raf:intervals,...rendered,fps:1000/rendered.mean_ms,renderer_cpu_ms:summarize(renderTimes)});
        }
        const canvas=document.querySelector('canvas');
        return {renderer:'Installed Project Graph 4.2.4 / CEF / Canvas2D',timing:'actual renderer.tick_ start interval; application scheduling limited, not GPU time; raf reported separately',nodes:entities.length,edges:p.stage.length-entities.length,moving_nodes:moving.length,window_css:[innerWidth,innerHeight],dpr:devicePixelRatio,canvas_pixels:[canvas.width,canvas.height],home:{x:home.x,y:home.y,scale},phases:results};
    } finally {
        p.renderer.tick_=oldRender;
        for(const node of moving)node.move(new Vector(-displacement.x,-displacement.y));
        camera.location=originalCamera.location;
        camera.targetLocationByScale=originalCamera.location.clone();
        camera.currentScale=originalCamera.currentScale;camera.targetScale=originalCamera.targetScale;
    }
})()
