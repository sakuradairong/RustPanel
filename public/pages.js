
// ── File Content Viewer ──
async function renderFileContent(){
  const c=document.getElementById('content');
  if(!c)return;
  const params=getHashParams();
  const path=params.path||'';
  if(!path){c.innerHTML='<div class="error-msg">No file path specified</div>';return}

  const FILE_KEY = md5('01b394cebf636ef53fe44c46a10abea2f54f60e6');

  c.innerHTML=`
  <div class="content-toolbar">
    <span style="flex:1;font-size:0.85rem;color:var(--text-muted);overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${escapeHtml(path)}</span>
    <button class="btn btn-sm btn-success" id="file-save-btn" disabled>Save</button>
    <button class="btn btn-sm btn-ghost" id="file-back-btn">Back</button>
  </div>
  <textarea class="content-area" id="file-content-area" spellcheck="false">Loading...</textarea>
  <div id="file-save-status" class="text-sm mt-2" style="color:var(--text-dim)"></div>`;

  document.getElementById('file-back-btn').addEventListener('click',()=>navigate('/file/list'));
  const area=document.getElementById('file-content-area');
  const saveBtn=document.getElementById('file-save-btn');
  const statusEl=document.getElementById('file-save-status');
  let originalContent='';

  try{
    const j=await api('/file/content?path='+encodeURIComponent(path));
    if(!j.success||!j.data){area.value='Failed to load file content';return}
    originalContent=sm4_cbc_decrypt_hex(j.data.content,FILE_KEY,FILE_KEY);
    area.value=originalContent;
    area.addEventListener('input',()=>{
      saveBtn.disabled=(area.value===originalContent);
    });
    saveBtn.disabled=false;
    saveBtn.addEventListener('click',async()=>{
      const newContent=area.value;
      if(newContent===originalContent){statusEl.textContent='No changes';return}
      saveBtn.disabled=true;saveBtn.textContent='Saving...';statusEl.textContent='';
      try{
        const encContent=sm4_cbc_encrypt_hex(newContent,FILE_KEY,FILE_KEY);
        const r=await api('/file/save',{
          method:'POST',
          body:JSON.stringify({path:path,content:encContent})
        });
        if(r.success){
          originalContent=newContent;
          statusEl.style.color='var(--success)';statusEl.textContent='Saved successfully';
          showToast('File saved','success');
          saveBtn.disabled=true;
        } else {
          statusEl.style.color='var(--error)';statusEl.textContent=r.message||'Save failed';
          saveBtn.disabled=false;
        }
      }catch(e){
        if(e.message!=='Unauthorized'){
          statusEl.style.color='var(--error)';statusEl.textContent='Error: '+e.message;
          saveBtn.disabled=false;
        }
      }
      saveBtn.textContent='Save';
    });
  }catch(e){
    if(e.message!=='Unauthorized')area.value='Error: '+e.message;
  }
}

// ── Docker Management ──
let dockerTab='containers';
async function renderDocker(){
  const c=document.getElementById('content');
  if(!c)return;
  cleanupPage();
  c.innerHTML=`
  <div class="flex justify-between items-center mb-4 flex-wrap gap-2">
    <h2 style="font-size:1.1rem;font-weight:600">Docker</h2>
    <div class="flex gap-2">
      <button class="btn btn-sm ${dockerTab==='containers'?'btn-success':'btn-ghost'}" id="dt-con">Containers</button>
      <button class="btn btn-sm ${dockerTab==='images'?'btn-success':'btn-ghost'}" id="dt-img">Images</button>
      <button class="btn btn-sm ${dockerTab==='volumes'?'btn-success':'btn-ghost'}" id="dt-vol">Volumes</button>
      <button class="btn btn-sm ${dockerTab==='networks'?'btn-success':'btn-ghost'}" id="dt-net">Networks</button>
    </div>
  </div>
  <div id="docker-content">Loading...</div>`;
  document.getElementById('dt-con').onclick=()=>{dockerTab='containers';renderDockerTab()};
  document.getElementById('dt-img').onclick=()=>{dockerTab='images';renderDockerTab()};
  document.getElementById('dt-img').onclick=()=>{dockerTab='images';renderDockerTab()};
  document.getElementById('dt-vol').onclick=()=>{dockerTab='volumes';renderDockerTab()};
  document.getElementById('dt-net').onclick=()=>{dockerTab='networks';renderDockerTab()};
  await renderDockerTab();
}
async function renderDockerTab(){
  const dc=document.getElementById('docker-content');
  if(!dc)return;
  if(dockerTab==='containers')await renderDockerContainers(dc);
  else if(dockerTab==='images')await renderDockerImages(dc);
  else if(dockerTab==='networks')await renderDockerNetworks(dc);
  else if(dockerTab==='volumes')await renderDockerVolumes(dc);
}

// ── Containers ──
let dcFilterQ='';
let dcFilterState='all'; // all | running | exited | other
let dcSelected=new Set();

async function renderDockerContainers(dc){
  dc.innerHTML='<div style="text-align:center;padding:2rem;color:var(--text-dim)">Loading...</div>';
  try{
    const j=await api('/docker/containers?all=true');
    if(!j.success){dc.innerHTML='<div class="error-msg">Failed</div>';return}
    const list=j.data||[];
    if(!Array.isArray(list)){dc.innerHTML='<div class="error-msg">Invalid data</div>';return}
    // Drop selections that no longer exist
    const ids=new Set(list.map(c=>c.id));
    dcSelected.forEach(id=>{if(!ids.has(id))dcSelected.delete(id)});

    const runningN=list.filter(c=>(c.state||'').toLowerCase()==='running').length;
    dc.innerHTML=`<div class="dk-toolbar">
      <div class="dk-toolbar-left">
        <input type="search" class="dk-search" id="dc-search" placeholder="Search name / image / port…" value="${escapeHtml(dcFilterQ)}" autocomplete="off">
        <div class="dk-chips" id="dc-chips">
          <button type="button" class="dk-chip${dcFilterState==='all'?' active':''}" data-st="all">All <span>${list.length}</span></button>
          <button type="button" class="dk-chip${dcFilterState==='running'?' active':''}" data-st="running">Running <span>${runningN}</span></button>
          <button type="button" class="dk-chip${dcFilterState==='exited'?' active':''}" data-st="exited">Exited</button>
          <button type="button" class="dk-chip${dcFilterState==='other'?' active':''}" data-st="other">Other</button>
        </div>
      </div>
      <div class="dk-toolbar-right">
        <span class="text-sm text-muted" id="dc-count"></span>
        <button class="btn btn-sm btn-success" id="dc-create-btn">+ Create</button>
        <button class="btn btn-sm btn-ghost" id="dc-refresh">⟳ Refresh</button>
      </div>
    </div>
    <div class="dk-batch" id="dc-batch">
      <span class="dk-batch-label" id="dc-batch-label">0 selected</span>
      <button class="btn btn-xs btn-success" id="dc-batch-start">Start</button>
      <button class="btn btn-xs btn-warning" id="dc-batch-stop">Stop</button>
      <button class="btn btn-xs btn-ghost" id="dc-batch-restart">Restart</button>
      <button class="btn btn-xs btn-danger" id="dc-batch-remove">Remove</button>
      <button class="btn btn-xs btn-ghost" id="dc-batch-clear">Clear</button>
    </div>
    <div id="dc-body"></div>`;

    const syncBatchBar=()=>{
      const bar=document.getElementById('dc-batch');
      const label=document.getElementById('dc-batch-label');
      if(!bar||!label)return;
      const n=dcSelected.size;
      label.textContent=n+' selected';
      bar.classList.toggle('show',n>0);
    };

    const runBatch=async(act)=>{
      const selected=[...dcSelected];
      if(!selected.length)return;
      if(act==='remove'){
        confirmDialog({title:'Remove Containers',messageHtml:'Remove <strong>'+selected.length+'</strong> container(s)? This cannot be undone.',okText:'Remove',danger:true,loadingText:'Removing...',onConfirm:async()=>{
          let ok=0,fail=0;
          for(const id of selected){
            const r=await api('/docker/containers/'+encodeURIComponent(id)+'/remove',{method:'POST'});
            if(r.success)ok++; else fail++;
          }
          dcSelected.clear();
          showToast(fail?('Removed '+ok+', failed '+fail):('Removed '+ok),'success');
          renderDockerContainers(dc);
        }});
        return;
      }
      let ok=0,fail=0;
      for(const id of selected){
        const r=await api('/docker/containers/'+encodeURIComponent(id)+'/'+act,{method:'POST'});
        if(r.success)ok++; else fail++;
      }
      showToast((act+' · ok '+ok+(fail?', fail '+fail:'')),fail?'error':'success');
      renderDockerContainers(dc);
    };

    const bindActs=()=>{
      dc.querySelectorAll('.dc-check').forEach(cb=>{
        cb.onchange=function(){
          const id=this.dataset.id;
          if(this.checked)dcSelected.add(id); else dcSelected.delete(id);
          this.closest('.dk-card')?.classList.toggle('selected',this.checked);
          syncBatchBar();
        };
      });
      dc.querySelectorAll('.dk-name[data-detail]').forEach(el=>{
        el.onclick=()=>openDockerDetailDrawer(dc,el.dataset.detail,el.dataset.name||'');
      });
      dc.querySelectorAll('.dc-act').forEach(btn=>{
        btn.onclick=async function(){
          const id=this.dataset.id;const act=this.dataset.act;
          if(!id)return;
          if(act==='logs'){renderDockerLogs(dc,id);return}
          if(act==='stats'){renderDockerStats(dc,id);return}
          if(act==='inspect'){openDockerDetailDrawer(dc,id,this.dataset.name||'');return}
          if(act==='exec'){openDockerExecModal(id,this.dataset.name||'');return}
          if(act==='term'){openDockerTerminal(id,this.dataset.name||'');return}
          if(act==='remove'){
            const nm=this.dataset.name||id.substring(0,12);
            confirmDialog({title:'Remove Container',messageHtml:'Remove container <strong>'+escapeHtml(nm)+'</strong>? This cannot be undone.',okText:'Remove',danger:true,loadingText:'Removing...',onConfirm:async()=>{
              const r=await api('/docker/containers/'+encodeURIComponent(id)+'/remove',{method:'POST'});
              if(!r.success)throw new Error(r.message||'Remove failed');
              dcSelected.delete(id);
              showToast('Container removed','success');renderDockerContainers(dc);
            }});
            return;
          }
          this.disabled=true;const orig=this.textContent;this.textContent='…';
          try{
            const r=await api('/docker/containers/'+encodeURIComponent(id)+'/'+act,{method:'POST'});
            if(!r.success)showToast((r.message||'Failed'),'error');
            renderDockerContainers(dc);
          }catch(e){if(e.message!=='Unauthorized')showToast('Error: '+e.message,'error');this.disabled=false;this.textContent=orig}
        };
      });
    };

    const paintBody=()=>{
      const q=dcFilterQ.trim().toLowerCase();
      const filtered=list.filter(ct=>{
        const st=(ct.state||'').toLowerCase();
        if(dcFilterState==='running'&&st!=='running')return false;
        if(dcFilterState==='exited'&&st!=='exited')return false;
        if(dcFilterState==='other'&&(st==='running'||st==='exited'))return false;
        if(!q)return true;
        const nm=(ct.name||'').replace(/^\//,'').toLowerCase();
        const img=(ct.image||'').toLowerCase();
        const id=(ct.id||'').toLowerCase();
        const ports=Array.isArray(ct.ports)?ct.ports.join(' ').toLowerCase():String(ct.ports||'').toLowerCase();
        return nm.includes(q)||img.includes(q)||id.includes(q)||ports.includes(q)||st.includes(q);
      });
      const countEl=document.getElementById('dc-count');
      if(countEl)countEl.textContent=filtered.length+' shown';
      document.querySelectorAll('#dc-chips .dk-chip').forEach(b=>b.classList.toggle('active',b.dataset.st===dcFilterState));
      const body=document.getElementById('dc-body');
      if(!body)return;
      if(list.length===0){
        body.innerHTML='<div class="empty-state"><div class="icon">🐳</div><div>No containers yet</div></div>';
        syncBatchBar();
        return;
      }
      if(filtered.length===0){
        body.innerHTML='<div class="empty-state"><div class="icon">🔎</div><div>No containers match this filter</div></div>';
        syncBatchBar();
        return;
      }
      let html='<div class="dk-grid">';
      filtered.forEach(ct=>{
        const shortId=(ct.id||'').substring(0,12);
        const st=ct.state||'';
        const sc=st.toLowerCase();
        const ports=Array.isArray(ct.ports)?ct.ports.join(', '):(ct.ports||'');
        const nm=(ct.name||'').replace(/^\//,'');
        const run=sc==='running';const paused=sc==='paused';
        const sel=dcSelected.has(ct.id);
        html+=`<div class="dk-card${sel?' selected':''}">
          <div class="dk-card-head">
            <input type="checkbox" class="dk-check dc-check" data-id="${ct.id}" ${sel?'checked':''} title="Select">
            <div class="dk-name" data-detail="${ct.id}" data-name="${escapeHtml(nm)}" title="Open details">${escapeHtml(nm||shortId)}</div>
            <span class="status-badge ${sc}">${escapeHtml(st)}</span>
          </div>
          <div class="dk-meta">
            <div class="dk-row"><span class="dk-k">Image</span><span class="dk-v" title="${escapeHtml(ct.image||'')}">${escapeHtml(ct.image||'—')}</span></div>
            <div class="dk-row"><span class="dk-k">ID</span><span class="dk-v mono">${escapeHtml(shortId)}</span></div>
            <div class="dk-row"><span class="dk-k">Ports</span><span class="dk-v">${escapeHtml(ports||'—')}</span></div>
            <div class="dk-row"><span class="dk-k">Status</span><span class="dk-v" title="${escapeHtml(ct.status||'')}">${escapeHtml(ct.status||'—')}</span></div>
          </div>
          <div class="dk-actions">
            <button class="btn btn-xs btn-success dc-act" data-id="${ct.id}" data-act="start" ${run||paused?'disabled':''} title="Start">▶</button>
            <button class="btn btn-xs btn-warning dc-act" data-id="${ct.id}" data-act="stop" ${!run?'disabled':''} title="Stop">⏹</button>
            <button class="btn btn-xs btn-ghost dc-act" data-id="${ct.id}" data-act="restart" title="Restart">↻</button>
            <button class="btn btn-xs btn-ghost dc-act" data-id="${ct.id}" data-act="pause" ${!run?'disabled':''} title="Pause">⏸</button>
            <button class="btn btn-xs btn-ghost dc-act" data-id="${ct.id}" data-act="unpause" ${!paused?'disabled':''} title="Unpause">⏯</button>
            <button class="btn btn-xs btn-ghost dc-act" data-id="${ct.id}" data-act="exec" data-name="${escapeHtml(nm)}" ${!run?'disabled':''} title="Exec">$</button>
            <button class="btn btn-xs btn-ghost dc-act" data-id="${ct.id}" data-act="term" data-name="${escapeHtml(nm)}" ${!run?'disabled':''} title="Terminal">⌨</button>
            <button class="btn btn-xs btn-ghost dc-act" data-id="${ct.id}" data-act="logs" title="Logs">📋</button>
            <button class="btn btn-xs btn-ghost dc-act" data-id="${ct.id}" data-act="stats" title="Stats">📊</button>
            <button class="btn btn-xs btn-ghost dc-act" data-id="${ct.id}" data-act="inspect" data-name="${escapeHtml(nm)}" title="Details">🔍</button>
            <button class="btn btn-xs btn-danger dc-act" data-id="${ct.id}" data-act="remove" data-name="${escapeHtml(nm)}" title="Remove">🗑</button>
          </div>
        </div>`;
      });
      html+='</div>';
      body.innerHTML=html;
      bindActs();
      syncBatchBar();
    };

    document.getElementById('dc-search').oninput=function(){dcFilterQ=this.value;paintBody()};
    document.getElementById('dc-chips').onclick=e=>{
      const btn=e.target.closest('.dk-chip');if(!btn)return;
      dcFilterState=btn.dataset.st||'all';paintBody();
    };
    document.getElementById('dc-refresh').onclick=()=>renderDockerContainers(dc);
    document.getElementById('dc-create-btn').onclick=()=>renderDockerCreate(dc);
    document.getElementById('dc-batch-start').onclick=()=>runBatch('start');
    document.getElementById('dc-batch-stop').onclick=()=>runBatch('stop');
    document.getElementById('dc-batch-restart').onclick=()=>runBatch('restart');
    document.getElementById('dc-batch-remove').onclick=()=>runBatch('remove');
    document.getElementById('dc-batch-clear').onclick=()=>{dcSelected.clear();paintBody()};
    paintBody();
  }catch(e){if(e.message!=='Unauthorized')dc.innerHTML='<div class="error-msg">Error: '+e.message+'</div>'}
}

function formatUptimeSecs(sec){
  sec=Number(sec)||0;
  if(sec<60)return sec+'s';
  if(sec<3600)return Math.floor(sec/60)+'m '+ (sec%60)+'s';
  if(sec<86400)return Math.floor(sec/3600)+'h '+Math.floor((sec%3600)/60)+'m';
  return Math.floor(sec/86400)+'d '+Math.floor((sec%86400)/3600)+'h';
}

async function openDockerDetailDrawer(dc,id,nameHint){
  const d=openDrawer('<div style="color:var(--text-dim);padding:1rem 0">Loading details…</div>',{title:nameHint||'Container',width:'480px'});
  try{
    const j=await api('/docker/containers/'+encodeURIComponent(id)+'/inspect');
    if(!j.success||!j.data){d.body.innerHTML='<div class="error-msg">Inspect unavailable</div>';return}
    const info=j.data;
    const cfg=info.Config||{};
    const st=info.State||{};
    const hc=info.HostConfig||{};
    const ns=info.NetworkSettings||{};
    const nm=(info.Name||nameHint||'').replace(/^\//,'');
    d.root.querySelector('.drawer-title').textContent=nm||id.substring(0,12);
    const env=(cfg.Env||[]).slice(0,20);
    const ports=ns.Ports||{};
    const portLines=Object.keys(ports).map(k=>{
      const binds=ports[k];
      if(!binds||!binds.length)return k;
      return binds.map(b=>(b.HostIp||'0.0.0.0')+':'+b.HostPort+'→'+k).join(', ');
    });
    const nets=ns.Networks||{};
    const netNames=Object.keys(nets);
    const restart=(hc.RestartPolicy&&hc.RestartPolicy.Name)||'—';
    const state=(st.Status||'').toLowerCase();
    const run=state==='running';
    const paused=state==='paused'||!!st.Paused;
    d.body.innerHTML=`
      <div class="drawer-section">
        <div style="display:flex;align-items:center;gap:.5rem;margin-bottom:.7rem">
          <span class="status-badge ${escapeHtml(state)}">${escapeHtml(st.Status||'—')}</span>
          <span class="text-sm text-muted">${escapeHtml(st.Error||'')}</span>
        </div>
        <div class="drawer-kv">
          <div class="k">ID</div><div class="v">${escapeHtml((info.Id||id).substring(0,12))}</div>
          <div class="k">Image</div><div class="v">${escapeHtml(cfg.Image||'—')}</div>
          <div class="k">Created</div><div class="v">${escapeHtml(info.Created||'—')}</div>
          <div class="k">Restart</div><div class="v">${escapeHtml(restart)}</div>
          <div class="k">Network</div><div class="v">${escapeHtml(netNames.join(', ')||hc.NetworkMode||'—')}</div>
          <div class="k">IP</div><div class="v">${escapeHtml(ns.IPAddress||(nets[netNames[0]]&&nets[netNames[0]].IPAddress)||'—')}</div>
          <div class="k">Cmd</div><div class="v">${escapeHtml((cfg.Cmd||[]).join(' ')||'—')}</div>
        </div>
      </div>
      <div class="drawer-section">
        <h4>Ports</h4>
        <div class="drawer-pre">${escapeHtml(portLines.join('\n')||'—')}</div>
      </div>
      <div class="drawer-section">
        <h4>Environment</h4>
        <div class="drawer-pre">${escapeHtml(env.join('\n')||'—')}${(cfg.Env||[]).length>20?'\n…':''}</div>
      </div>
      <div class="drawer-section">
        <h4>Actions</h4>
        <div class="drawer-actions">
          <button class="btn btn-sm btn-success" id="dr-start" ${run||paused?'disabled':''}>Start</button>
          <button class="btn btn-sm btn-warning" id="dr-stop" ${!run?'disabled':''}>Stop</button>
          <button class="btn btn-sm btn-ghost" id="dr-restart">Restart</button>
          <button class="btn btn-sm btn-ghost" id="dr-pause" ${!run?'disabled':''}>Pause</button>
          <button class="btn btn-sm btn-ghost" id="dr-unpause" ${!paused?'disabled':''}>Unpause</button>
          <button class="btn btn-sm btn-ghost" id="dr-exec" ${!run?'disabled':''}>Exec</button>
          <button class="btn btn-sm btn-ghost" id="dr-term" ${!run?'disabled':''}>Terminal</button>
          <button class="btn btn-sm btn-ghost" id="dr-logs">Logs</button>
          <button class="btn btn-sm btn-ghost" id="dr-stats">Stats</button>
          <button class="btn btn-sm btn-ghost" id="dr-raw">Raw JSON</button>
          <button class="btn btn-sm btn-danger" id="dr-remove">Remove</button>
        </div>
      </div>
      <div class="drawer-section" id="dr-raw-box" style="display:none">
        <h4>Raw Inspect</h4>
        <pre class="drawer-pre" style="max-height:360px">${escapeHtml(JSON.stringify(info,null,2))}</pre>
      </div>`;
    const act=async(a)=>{
      const r=await api('/docker/containers/'+encodeURIComponent(id)+'/'+a,{method:'POST'});
      if(!r.success){showToast(r.message||'Failed','error');return}
      showToast(a+' ok','success');d.close();renderDockerContainers(dc);
    };
    d.body.querySelector('#dr-start').onclick=()=>act('start');
    d.body.querySelector('#dr-stop').onclick=()=>act('stop');
    d.body.querySelector('#dr-restart').onclick=()=>act('restart');
    d.body.querySelector('#dr-pause').onclick=()=>act('pause');
    d.body.querySelector('#dr-unpause').onclick=()=>act('unpause');
    d.body.querySelector('#dr-exec').onclick=()=>{d.close();openDockerExecModal(id,nm)};
    d.body.querySelector('#dr-term').onclick=()=>{d.close();openDockerTerminal(id,nm)};
    d.body.querySelector('#dr-logs').onclick=()=>{d.close();renderDockerLogs(dc,id)};
    d.body.querySelector('#dr-stats').onclick=()=>{d.close();renderDockerStats(dc,id)};
    d.body.querySelector('#dr-raw').onclick=()=>{
      const box=d.body.querySelector('#dr-raw-box');
      box.style.display=box.style.display==='none'?'block':'none';
    };
    d.body.querySelector('#dr-remove').onclick=()=>{
      confirmDialog({title:'Remove Container',messageHtml:'Remove <strong>'+escapeHtml(nm)+'</strong>?',okText:'Remove',danger:true,loadingText:'Removing...',onConfirm:async()=>{
        const r=await api('/docker/containers/'+encodeURIComponent(id)+'/remove',{method:'POST'});
        if(!r.success)throw new Error(r.message||'Remove failed');
        showToast('Removed','success');d.close();renderDockerContainers(dc);
      }});
    };
  }catch(e){
    if(e.message!=='Unauthorized')d.body.innerHTML='<div class="error-msg">Error: '+escapeHtml(e.message)+'</div>';
  }
}

async function renderDockerLogs(dc,id){
  await loadDockerLogs(dc,id,200,false);
}
async function loadDockerLogs(dc,id,tail,silent){
  let pre=dc.querySelector('#dc-log-pre');
  const nearBottom=pre?(pre.scrollHeight-pre.scrollTop-pre.clientHeight)<48:true;
  const prevScroll=pre?pre.scrollTop:0;
  if(!silent||!pre){
    if(!silent)dc.innerHTML='<div style="text-align:center;padding:2rem;color:var(--text-dim)">Loading logs...</div>';
  } else {
    const st=dc.querySelector('#dc-log-status');
    if(st)st.textContent='Refreshing…';
  }
  try{
    const j=await api('/docker/containers/'+encodeURIComponent(id)+'/logs?tail='+tail);
    const logs=j.data?.logs||'';
    if(!silent||!dc.querySelector('#dc-log-pre')){
      dc.innerHTML=`
      <div class="flex justify-between items-center mb-2">
        <span style="font-size:0.85rem;font-weight:600">Logs</span>
        <div class="flex items-center gap-2">
          <span class="text-sm text-muted" id="dc-log-status"></span>
          <span class="text-sm text-muted">Lines:</span>
          <select id="dc-log-tail" style="width:auto;padding:2px 8px">
            <option value="50" ${tail==50?'selected':''}>50</option>
            <option value="200" ${tail==200?'selected':''}>200</option>
            <option value="1000" ${tail==1000?'selected':''}>1000</option>
            <option value="5000" ${tail==5000?'selected':''}>5000</option>
          </select>
          <button class="btn btn-sm btn-ghost" id="dc-logs-refresh">⟳</button>
          <button class="btn btn-sm" id="dc-logs-back">Back</button>
        </div>
      </div>
      <pre id="dc-log-pre" style="background:#1a1a2e;color:#e0e0e0;padding:12px;border-radius:6px;font-size:0.8rem;max-height:600px;overflow-y:auto;font-family:monospace;white-space:pre-wrap"></pre>`;
      document.getElementById('dc-log-tail').onchange=function(){loadDockerLogs(dc,id,this.value,false)};
      document.getElementById('dc-logs-refresh').onclick=()=>loadDockerLogs(dc,id,document.getElementById('dc-log-tail').value,true);
      document.getElementById('dc-logs-back').onclick=()=>renderDockerContainers(dc);
      pre=document.getElementById('dc-log-pre');
    }
    pre.textContent=logs||'No logs';
    if(!silent||nearBottom)pre.scrollTop=pre.scrollHeight;
    else pre.scrollTop=prevScroll;
    const st=dc.querySelector('#dc-log-status');
    if(st)st.textContent='Updated '+new Date().toLocaleTimeString();
  }catch(e){if(e.message!=='Unauthorized')dc.innerHTML='<div class="error-msg">Error: '+e.message+'</div>'}
}

// ── Container Stats ──
async function renderDockerStats(dc,id){
  dc.innerHTML='<div style="text-align:center;padding:2rem;color:var(--text-dim)">Loading stats...</div>';
  try{
    const j=await api('/docker/containers/'+encodeURIComponent(id)+'/stats');
    if(!j.success||!j.data){dc.innerHTML='<div class="error-msg">Stats unavailable</div>';return}
    const d=j.data;
    const mem=d.memory_stats||{};
    const cpu=d.cpu_stats||{};
    const precpu=d.precpu_stats||{};
    const nets=d.networks||{};
    const blk=d.blkio_stats||{};
    const pidsS=d.pids_stats||{};
    const memUsed=mem.usage||0;
    const memLimit=mem.limit||1;
    const memPct=memLimit>0?((memUsed/memLimit)*100).toFixed(1)+'%':'N/A';
    const memStr=formatSize(memUsed)+' / '+formatSize(memLimit);
    const cpuTotal=cpu.cpu_usage?.total_usage||0;
    const cpuPrev=precpu.cpu_usage?.total_usage||0;
    const sysTotal=cpu.system_cpu_usage||0;
    const sysPrev=precpu.system_cpu_usage||0;
    const cpuDelta=cpuTotal-cpuPrev;
    const sysDelta=sysTotal-sysPrev;
    const onlineCpus=cpu.online_cpus||1;
    const cpuPct=sysDelta>0?Math.min(100,((cpuDelta/sysDelta)*onlineCpus*100).toFixed(1))+'%':'N/A';
    let netRx=0,netTx=0;
    Object.values(nets).forEach(n=>{netRx+=n.rx_bytes||0;netTx+=n.tx_bytes||0;});
    const netRxStr=netRx>0?formatSize(netRx)+'/s':'N/A';
    const netTxStr=netTx>0?formatSize(netTx)+'/s':'N/A';
    let blkRead=0,blkWrite=0;
    (blk.io_service_bytes_recursive||[]).forEach(o=>{
      if(o.op==='read')blkRead+=o.value||0;
      if(o.op==='write')blkWrite+=o.value||0;
    });
    const pidStr=pidsS.current!=null?'PIDs: '+pidsS.current:'';
    dc.innerHTML=`
    <div class="flex justify-between items-center mb-3">
      <span style="font-size:0.85rem;font-weight:600">Container Stats</span>
      <button class="btn btn-sm" id="dc-stats-back">Back</button>
    </div>
    <div class="monitor-grid">
      <div class="card monitor-card"><div class="label">CPU</div><div class="value" style="font-size:1.5rem">${cpuPct}</div><div class="sub-value">${pidStr}</div></div>
      <div class="card monitor-card"><div class="label">Memory</div><div class="value" style="font-size:1.25rem">${memPct}</div><div class="sub-value">${memStr}</div></div>
      <div class="card monitor-card"><div class="label">Network RX</div><div class="value" style="font-size:1.25rem">${netRxStr}</div></div>
      <div class="card monitor-card"><div class="label">Network TX</div><div class="value" style="font-size:1.25rem">${netTxStr}</div></div>
    </div>`;
    if(blkRead>0||blkWrite>0){
      dc.innerHTML+=`<div class="card mt-3"><h3 style="font-size:0.9rem;margin-bottom:.5rem;color:var(--text-muted)">Block I/O</h3><div class="text-sm">Read: ${formatSize(blkRead)} | Write: ${formatSize(blkWrite)}</div></div>`;
    }
    document.getElementById('dc-stats-back').onclick=()=>renderDockerContainers(dc);
  }catch(e){if(e.message!=='Unauthorized')dc.innerHTML='<div class="error-msg">Error: '+e.message+'</div>'}
}

// ── Container Inspect (legacy full-page; prefer drawer) ──
async function renderDockerInspect(dc,id){
  openDockerDetailDrawer(dc,id,'');
}

function openDockerExecModal(id,nameHint){
  const title=nameHint?('Exec · '+nameHint):'Exec Command';
  const m=openModal(`
    <h3>${escapeHtml(title)}</h3>
    <div class="field"><label>Command</label>
      <input type="text" id="dc-exec-cmd" placeholder="ls -la /" value="uname -a" list="dc-exec-presets">
      <datalist id="dc-exec-presets">
        <option value="uname -a"></option>
        <option value="cat /etc/os-release"></option>
        <option value="ps aux"></option>
        <option value="df -h"></option>
        <option value="env"></option>
      </datalist>
    </div>
    <div class="text-xs text-dim" style="margin:-4px 0 8px">One-shot via <code>/bin/sh -c</code>. For a live shell, use <strong>Terminal</strong>.</div>
    <div class="flex justify-between items-center mb-1">
      <span class="text-sm text-muted">Output</span>
      <button class="btn btn-xs btn-ghost" id="dc-exec-copy" type="button">Copy</button>
    </div>
    <pre id="dc-exec-out" class="drawer-pre" style="min-height:120px;max-height:280px">Output will appear here…</pre>
    <div id="dc-exec-err" class="error-msg" style="display:none"></div>
    <div class="btn-row">
      <button class="btn btn-sm btn-ghost" id="dc-exec-close">Close</button>
      <button class="btn btn-sm btn-ghost" id="dc-exec-term">Open Terminal</button>
      <button class="btn btn-sm btn-success" id="dc-exec-run">Run</button>
    </div>`,{maxWidth:'560px'});
  m.root.querySelector('#dc-exec-close').onclick=m.close;
  m.root.querySelector('#dc-exec-term').onclick=()=>{m.close();openDockerTerminal(id,nameHint)};
  m.root.querySelector('#dc-exec-copy').onclick=async()=>{
    const t=m.root.querySelector('#dc-exec-out').textContent||'';
    try{await navigator.clipboard.writeText(t);showToast('Copied','success')}
    catch(e){showToast('Copy failed','error')}
  };
  const run=async()=>{
    const cmd=m.root.querySelector('#dc-exec-cmd').value.trim();
    const out=m.root.querySelector('#dc-exec-out');
    const err=m.root.querySelector('#dc-exec-err');
    const btn=m.root.querySelector('#dc-exec-run');
    if(!cmd){err.textContent='Command required';err.style.display='block';return}
    err.style.display='none';btn.disabled=true;btn.textContent='Running…';out.textContent='…';
    try{
      const r=await api('/docker/containers/'+encodeURIComponent(id)+'/exec',{method:'POST',body:JSON.stringify({cmd})});
      if(r.success){
        out.textContent=(r.data&&r.data.output!=null)?String(r.data.output):'(no output)';
        showToast('Exec finished','success');
      } else {
        err.textContent=r.message||'Exec failed';err.style.display='block';
        out.textContent='';
      }
    }catch(e){
      if(e.message==='Unauthorized'){m.close();return}
      err.textContent='Error: '+e.message;err.style.display='block';
    }finally{
      btn.disabled=false;btn.textContent='Run';
    }
  };
  m.root.querySelector('#dc-exec-run').onclick=run;
  m.root.querySelector('#dc-exec-cmd').addEventListener('keydown',e=>{if(e.key==='Enter')run()});
}

function openDockerTerminal(id,nameHint){
  const title=nameHint?('Terminal · '+nameHint):'Terminal';
  const m=openModal(`
    <h3>${escapeHtml(title)}</h3>
    <div class="flex justify-between items-center mb-2">
      <span class="text-sm text-muted" id="dc-term-status">Connecting…</span>
      <select id="dc-term-shell" style="width:auto;padding:2px 8px">
        <option value="/bin/sh">/bin/sh</option>
        <option value="/bin/bash">/bin/bash</option>
      </select>
    </div>
    <pre id="dc-term-out" class="drawer-pre term-screen" style="min-height:280px;max-height:420px;background:#0b1220;color:#d1fae5"></pre>
    <div class="flex gap-2 mt-2">
      <input type="text" id="dc-term-in" placeholder="Type a command and press Enter" style="flex:1" autocomplete="off" spellcheck="false">
      <button class="btn btn-sm btn-success" id="dc-term-send">Send</button>
    </div>
    <div class="btn-row">
      <button class="btn btn-sm btn-ghost" id="dc-term-clear">Clear</button>
      <button class="btn btn-sm btn-ghost" id="dc-term-close">Close</button>
    </div>`,{maxWidth:'720px',onClose:()=>{try{if(ws&&ws.readyState<=1)ws.close()}catch(e){}}});
  const out=m.root.querySelector('#dc-term-out');
  const inp=m.root.querySelector('#dc-term-in');
  const status=m.root.querySelector('#dc-term-status');
  const append=(t)=>{
    // Strip common TTY noise (cursor position report) so prompts stay readable without xterm.js
    const cleaned=String(t||'').replace(/\x1b\[[0-9;]*[A-Za-z]/g,'');
    if(!cleaned)return;
    out.textContent+=cleaned;out.scrollTop=out.scrollHeight;
  };
  let ws=null;
  const connect=()=>{
    try{if(ws)ws.close()}catch(e){}
    out.textContent='';
    const token=getToken()||'';
    const shell=m.root.querySelector('#dc-term-shell').value||'/bin/sh';
    const proto=location.protocol==='https:'?'wss:':'ws:';
    const url=proto+'//'+location.host+'/api/v1/docker/containers/'+encodeURIComponent(id)+'/terminal?token='+encodeURIComponent(token)+'&shell='+encodeURIComponent(shell);
    status.textContent='Connecting…';status.style.color='var(--text-dim)';
    ws=new WebSocket(url);
    ws.onopen=()=>{status.textContent='Connected · '+shell;status.style.color='var(--success)';inp.focus()};
    ws.onmessage=(ev)=>{append(typeof ev.data==='string'?ev.data:String(ev.data))};
    ws.onerror=()=>{status.textContent='Error';status.style.color='var(--error)'};
    ws.onclose=()=>{status.textContent='Disconnected';status.style.color='var(--warning)'};
  };
  const send=()=>{
    const line=inp.value;
    if(ws&&ws.readyState===1){
      ws.send(line+'\n');
      append(line+'\n');
      inp.value='';
    } else showToast('Terminal not connected','error');
  };
  m.root.querySelector('#dc-term-send').onclick=send;
  inp.addEventListener('keydown',e=>{if(e.key==='Enter'){e.preventDefault();send()}});
  m.root.querySelector('#dc-term-clear').onclick=()=>{out.textContent=''};
  m.root.querySelector('#dc-term-close').onclick=()=>{try{if(ws)ws.close()}catch(e){}m.close()};
  m.root.querySelector('#dc-term-shell').onchange=connect;
  connect();
}

function renderDockerCreate(dc){
  const m=openModal(`
    <h3>Create Container</h3>
    <div class="field"><label>Image</label><input type="text" id="dc-img" placeholder="nginx:latest"></div>
    <div class="field"><label>Name</label><input type="text" id="dc-name" placeholder="my-nginx"></div>
    <div class="field"><label>Ports <span class="text-dim" style="font-weight:400">(host:container, comma-separated)</span></label><input type="text" id="dc-ports" placeholder="8080:80, 5432:5432/tcp"></div>
    <div class="field"><label>Environment <span class="text-dim" style="font-weight:400">(KEY=VALUE, one per line)</span></label><textarea id="dc-env" rows="3" placeholder="TZ=UTC&#10;DEBUG=1" style="resize:vertical"></textarea></div>
    <div class="field"><label>Restart Policy</label>
      <select id="dc-restart">
        <option value="no">no</option>
        <option value="unless-stopped" selected>unless-stopped</option>
        <option value="always">always</option>
        <option value="on-failure">on-failure</option>
      </select>
    </div>
    <div class="field"><label>Network <span class="text-dim" style="font-weight:400">(bridge / host / custom)</span></label><input type="text" id="dc-network" placeholder="bridge" value="bridge"></div>
    <div class="field"><label>Command <span class="text-dim" style="font-weight:400">(optional)</span></label><input type="text" id="dc-cmd" placeholder="optional"></div>
    <div id="dc-err" class="error-msg" style="display:none"></div>
    <div class="btn-row">
      <button class="btn btn-sm btn-ghost" id="dc-c-back">Cancel</button>
      <button class="btn btn-sm btn-success" id="dc-c-do">Create & Start</button>
    </div>`,{maxWidth:'480px'});
  m.root.querySelector('#dc-c-back').addEventListener('click',m.close);
  m.root.querySelector('#dc-c-do').addEventListener('click',async function(){
    const img=m.root.querySelector('#dc-img').value.trim();
    const nm=m.root.querySelector('#dc-name').value.trim();
    const cmdStr=m.root.querySelector('#dc-cmd').value.trim();
    const portsStr=m.root.querySelector('#dc-ports').value.trim();
    const envStr=m.root.querySelector('#dc-env').value.trim();
    const restart=m.root.querySelector('#dc-restart').value;
    const network=m.root.querySelector('#dc-network').value.trim();
    const err=m.root.querySelector('#dc-err');
    if(!img||!nm){err.textContent='Image and name required';err.style.display='block';return}
    err.style.display='none';
    this.disabled=true;this.textContent='Creating...';
    try{
      const body={name:nm,image:img};
      if(cmdStr)body.cmd=cmdStr.split(/\s+/);
      const ports=portsStr.split(/[\n,]+/).map(s=>s.trim()).filter(Boolean);
      if(ports.length)body.ports=ports;
      const env=envStr.split(/\n+/).map(s=>s.trim()).filter(Boolean);
      if(env.length)body.env=env;
      if(restart)body.restart_policy=restart;
      if(network)body.network_mode=network;
      const r=await api('/docker/containers',{method:'POST',body:JSON.stringify(body)});
      if(r.success){
        showToast('Created: '+nm,'success');
        if(r.data&&r.data.container_id) await api('/docker/containers/'+encodeURIComponent(r.data.container_id)+'/start',{method:'POST'});
        m.close();renderDockerContainers(dc);
      } else {err.textContent=r.message||'Failed';err.style.display='block';this.disabled=false;this.textContent='Create & Start'}
    }catch(e){if(e.message==='Unauthorized'){m.close();return}err.textContent='Error: '+e.message;err.style.display='block';this.disabled=false;this.textContent='Create & Start'}
  });
}

// ── Images ──
let diFilterQ='';
async function renderDockerImages(dc){
  dc.innerHTML='<div style="text-align:center;padding:2rem;color:var(--text-dim)">Loading images...</div>';
  try{
    const j=await api('/docker/images');
    if(!j.success){dc.innerHTML='<div class="error-msg">Failed</div>';return}
    const list=j.data||[];if(!Array.isArray(list)){dc.innerHTML='<div class="error-msg">Invalid</div>';return}
    dc.innerHTML=`<div class="dk-toolbar">
      <div class="dk-toolbar-left">
        <input type="search" class="dk-search" id="di-search" placeholder="Search image…" value="${escapeHtml(diFilterQ)}" autocomplete="off">
        <span class="text-sm text-muted" id="di-count"></span>
      </div>
      <div class="dk-toolbar-right">
        <button class="btn btn-sm btn-ghost" id="di-prune" title="Remove dangling images">Prune</button>
        <button class="btn btn-sm btn-success" id="di-pull">+ Pull</button>
      </div>
    </div>
    <div id="di-body"></div>`;
    const paint=()=>{
      const q=diFilterQ.trim().toLowerCase();
      const filtered=list.filter(img=>{
        if(!q)return true;
        const tags=(img.repo_tags||[]).join(' ').toLowerCase();
        const id=(img.id||'').toLowerCase();
        return tags.includes(q)||id.includes(q);
      });
      document.getElementById('di-count').textContent=filtered.length+' / '+list.length+' image(s)';
      const body=document.getElementById('di-body');
      if(list.length===0){body.innerHTML='<div class="empty-state"><div class="icon">📦</div><div>No images yet</div></div>';return}
      if(filtered.length===0){body.innerHTML='<div class="empty-state"><div class="icon">🔎</div><div>No images match</div></div>';return}
      let html='<div class="dk-grid">';
      filtered.forEach(img=>{
        const tags=img.repo_tags||[];
        const full=tags.length>0?tags[0]:'<none>:<none>';
        const repo=tags.length>0?(tags[0].split(':')[0]||'<none>'):'<none>';
        const tag=tags.length>0?(tags[0].split(':')[1]||'latest'):'<none>';
        const sid=(img.id||'').replace('sha256:','').substring(0,12);
        const sz=img.size||0;
        const szStr=sz>1073741824?(sz/1073741824).toFixed(2)+' GB':sz>1048576?(sz/1048576).toFixed(1)+' MB':(sz/1024).toFixed(0)+' KB';
        html+=`<div class="dk-card">
          <div class="dk-card-head">
            <div class="dk-name" title="${escapeHtml(full)}">${escapeHtml(repo)}</div>
            <span class="app-cat">${escapeHtml(tag)}</span>
          </div>
          <div class="dk-meta">
            <div class="dk-row"><span class="dk-k">ID</span><span class="dk-v mono">${escapeHtml(sid)}</span></div>
            <div class="dk-row"><span class="dk-k">Size</span><span class="dk-v">${szStr}</span></div>
          </div>
          <div class="dk-actions">
            <button class="btn btn-xs btn-danger di-rm" data-id="${escapeHtml(img.id||'')}" data-name="${escapeHtml(full)}" title="Remove">🗑 Remove</button>
          </div>
        </div>`;
      });
      html+='</div>';
      body.innerHTML=html;
      body.querySelectorAll('.di-rm').forEach(btn=>{btn.onclick=function(){
        const id=this.dataset.id;const name=this.dataset.name||id;
        confirmDialog({title:'Remove Image',messageHtml:'Remove image <strong>'+escapeHtml(name)+'</strong>?',okText:'Remove',danger:true,loadingText:'Removing...',onConfirm:async()=>{
          const r=await api('/docker/images/'+encodeURIComponent(id)+'/remove',{method:'POST'});
          if(!r.success)throw new Error(r.message||'Remove failed');
          showToast('Removed','success');renderDockerImages(dc);
        }});
      };});
    };
    document.getElementById('di-search').oninput=function(){diFilterQ=this.value;paint()};
    document.getElementById('di-pull').onclick=()=>renderDockerPull(dc);
    document.getElementById('di-prune').onclick=()=>{
      const m=openModal(`
        <h3>Prune Images</h3>
        <p style="color:var(--text-muted);font-size:.9rem;margin-bottom:1rem">Choose what to clean up. This cannot be undone.</p>
        <div class="field"><label>Scope</label>
          <select id="di-prune-scope">
            <option value="dangling">Dangling only (untagged unused)</option>
            <option value="all">All unused images</option>
          </select>
        </div>
        <div id="di-prune-err" class="error-msg" style="display:none"></div>
        <div class="btn-row">
          <button class="btn btn-sm btn-ghost" id="di-prune-cancel">Cancel</button>
          <button class="btn btn-sm btn-danger" id="di-prune-go">Prune</button>
        </div>`,{maxWidth:'440px'});
      m.root.querySelector('#di-prune-cancel').onclick=m.close;
      m.root.querySelector('#di-prune-go').onclick=async function(){
        const dangling=m.root.querySelector('#di-prune-scope').value!=='all';
        const err=m.root.querySelector('#di-prune-err');
        err.style.display='none';this.disabled=true;this.textContent='Pruning…';
        try{
          const r=await api('/docker/images/prune',{method:'POST',body:JSON.stringify({dangling_only:dangling})});
          if(!r.success)throw new Error(r.message||'Prune failed');
          const d=r.data||{};
          const freed=d.space_reclaimed!=null?formatSize(d.space_reclaimed):'0 B';
          showToast('Pruned '+(d.deleted||0)+' · freed '+freed,'success');
          m.close();renderDockerImages(dc);
        }catch(e){
          if(e.message==='Unauthorized'){m.close();return}
          err.textContent=e.message;err.style.display='block';
          this.disabled=false;this.textContent='Prune';
        }
      };
    };
    paint();
  }catch(e){if(e.message!=='Unauthorized')dc.innerHTML='<div class="error-msg">Error: '+e.message+'</div>'}
}

function renderDockerPull(dc){
  const m=openModal(`
    <h3>Pull Image</h3>
    <div class="field"><label>Image</label><input type="text" id="di-pull-img" placeholder="nginx:latest"></div>
    <div id="di-pull-err" class="error-msg" style="display:none"></div>
    <div class="btn-row">
      <button class="btn btn-sm btn-ghost" id="di-pull-back">Cancel</button>
      <button class="btn btn-sm btn-success" id="di-pull-go">Pull</button>
    </div>`,{maxWidth:'460px'});
  m.root.querySelector('#di-pull-back').addEventListener('click',()=>{m.close();renderDockerImages(dc)});
  m.root.querySelector('#di-pull-go').addEventListener('click',async function(){
    const img=m.root.querySelector('#di-pull-img').value.trim();
    const err=m.root.querySelector('#di-pull-err');
    if(!img){err.textContent='Image required';err.style.display='block';return}
    err.style.display='none';this.disabled=true;this.textContent='Pulling…';
    try{
      const r=await api('/docker/images/pull',{method:'POST',body:JSON.stringify({image:img})});
      if(r.success){showToast('Pulled: '+img,'success');m.close();renderDockerImages(dc)}
      else{err.textContent=r.message||'Failed';err.style.display='block';this.disabled=false;this.textContent='Pull'}
    }catch(e){if(e.message==='Unauthorized'){m.close();return}err.textContent='Error: '+e.message;err.style.display='block';this.disabled=false;this.textContent='Pull'}
  });
}

// ── Networks ──
async function renderDockerNetworks(dc){
  dc.innerHTML='<div style="text-align:center;padding:2rem;color:var(--text-dim)">Loading networks...</div>';
  try{
    const j=await api('/docker/networks');
    if(!j.success){dc.innerHTML='<div class="error-msg">Failed</div>';return}
    const list=j.data||[];if(!Array.isArray(list)){dc.innerHTML='<div class="error-msg">Invalid</div>';return}
    let html='<div class="flex justify-between items-center mb-3"><span style="font-size:0.85rem;color:var(--text-dim)">'+list.length+' network(s)</span><button class="btn btn-sm btn-success" id="dn-add">+ Create</button></div>';
    if(list.length===0){
      html+='<div class="empty-state"><div class="icon">🔗</div><div>No networks</div></div>';
      dc.innerHTML=html;document.getElementById('dn-add').onclick=()=>renderDockerNetworkCreate(dc);return;
    }
    html+='<div class="dk-grid">';
    list.forEach(n=>{
      const isDefault=['bridge','host','none'].indexOf(n.name)>=0;
      html+=`<div class="dk-card">
        <div class="dk-card-head">
          <div class="dk-name" title="${escapeHtml(n.name||'')}">${escapeHtml(n.name||'')}</div>
          <span class="app-cat">${escapeHtml(n.driver||'')}</span>
        </div>
        <div class="dk-meta">
          <div class="dk-row"><span class="dk-k">Scope</span><span class="dk-v">${escapeHtml(n.scope||'—')}</span></div>
          <div class="dk-row"><span class="dk-k">Subnet</span><span class="dk-v mono">${escapeHtml(n.subnet||'—')}</span></div>
          <div class="dk-row"><span class="dk-k">Gateway</span><span class="dk-v mono">${escapeHtml(n.gateway||'—')}</span></div>
        </div>
        <div class="dk-actions">
          <button class="btn btn-xs btn-danger dn-rm" data-id="${escapeHtml(n.id||'')}" data-name="${escapeHtml(n.name||'')}" ${isDefault?'disabled title="Built-in network"':'title="Remove"'}>🗑 Remove</button>
        </div>
      </div>`;
    });
    html+='</div>';
    dc.innerHTML=html;
    document.getElementById('dn-add').onclick=()=>renderDockerNetworkCreate(dc);
    dc.querySelectorAll('.dn-rm').forEach(btn=>{btn.onclick=function(){
      const id=this.dataset.id;const name=this.dataset.name||id;
      confirmDialog({title:'Remove Network',messageHtml:'Remove network <strong>'+escapeHtml(name)+'</strong>?',okText:'Remove',danger:true,loadingText:'Removing...',onConfirm:async()=>{
        const r=await api('/docker/networks/'+encodeURIComponent(id)+'/remove',{method:'POST'});
        if(!r.success)throw new Error(r.message||'Remove failed');
        showToast('Removed','success');renderDockerNetworks(dc);
      }});
    };});
  }catch(e){if(e.message!=='Unauthorized')dc.innerHTML='<div class="error-msg">Error: '+e.message+'</div>'}
}

function renderDockerNetworkCreate(dc){
  const m=openModal(`
    <h3>Create Network</h3>
    <div class="field"><label>Name</label><input type="text" id="dn-name" placeholder="my-network"></div>
    <div class="field"><label>Driver</label><select id="dn-driver"><option value="bridge">bridge</option><option value="overlay">overlay</option><option value="macvlan">macvlan</option><option value="host">host</option></select></div>
    <div class="form-row"><div class="field"><label>Subnet</label><input type="text" id="dn-sub" placeholder="172.20.0.0/16"></div><div class="field"><label>Gateway</label><input type="text" id="dn-gw" placeholder="172.20.0.1"></div></div>
    <div id="dn-err" class="error-msg" style="display:none"></div>
    <div class="btn-row">
      <button class="btn btn-sm btn-ghost" id="dn-c-back">Cancel</button>
      <button class="btn btn-sm btn-success" id="dn-c-do">Create</button>
    </div>`,{maxWidth:'480px'});
  m.root.querySelector('#dn-c-back').addEventListener('click',m.close);
  m.root.querySelector('#dn-c-do').addEventListener('click',async function(){
    const nm=m.root.querySelector('#dn-name').value.trim();
    const drv=m.root.querySelector('#dn-driver').value;
    const sub=m.root.querySelector('#dn-sub').value.trim();
    const gw=m.root.querySelector('#dn-gw').value.trim();
    const err=m.root.querySelector('#dn-err');
    if(!nm){err.textContent='Name required';err.style.display='block';return}
    err.style.display='none';this.disabled=true;this.textContent='Creating...';
    try{
      const body={name:nm,driver:drv};if(sub)body.subnet=sub;if(gw)body.gateway=gw;
      const r=await api('/docker/networks',{method:'POST',body:JSON.stringify(body)});
      if(r.success){showToast('Created','success');m.close();renderDockerNetworks(dc)}
      else{err.textContent=r.message||'Failed';err.style.display='block';this.disabled=false;this.textContent='Create'}
    }catch(e){if(e.message==='Unauthorized'){m.close();return}err.textContent='Error: '+e.message;err.style.display='block';this.disabled=false;this.textContent='Create'}
  });
}

// ── Volumes ──
async function renderDockerVolumes(dc){
  dc.innerHTML='<div style="text-align:center;padding:2rem;color:var(--text-dim)">Loading volumes...</div>';
  try{
    const j=await api('/docker/volumes');
    if(!j.success){dc.innerHTML='<div class="error-msg">Failed</div>';return}
    const list=j.data||[];if(!Array.isArray(list)){dc.innerHTML='<div class="error-msg">Invalid</div>';return}
    dc.innerHTML=`<div class="dk-toolbar">
      <div class="dk-toolbar-left">
        <span class="text-sm text-muted" id="dv-count">${list.length} volume(s)</span>
      </div>
      <div class="dk-toolbar-right">
        <button class="btn btn-sm btn-ghost" id="dv-prune" title="Remove unused volumes">Prune</button>
        <button class="btn btn-sm btn-success" id="dv-add">+ Create</button>
      </div>
    </div>
    <div id="dv-body"></div>`;
    const body=document.getElementById('dv-body');
    if(list.length===0){
      body.innerHTML='<div class="empty-state"><div class="icon">💾</div><div>No volumes</div></div>';
    } else {
      let html='<div class="dk-grid">';
      list.forEach(v=>{
        const nm=v.name||'';
        const mp=v.mountpoint||'';
        const sz=v.size>0?formatSize(v.size):'—';
        html+=`<div class="dk-card">
          <div class="dk-card-head">
            <div class="dk-name" title="${escapeHtml(nm)}">${escapeHtml(nm)}</div>
            <span class="app-cat">${escapeHtml(v.driver||'local')}</span>
          </div>
          <div class="dk-meta">
            <div class="dk-row"><span class="dk-k">Scope</span><span class="dk-v">${escapeHtml(v.scope||'—')}</span></div>
            <div class="dk-row"><span class="dk-k">Size</span><span class="dk-v">${sz}</span></div>
            <div class="dk-row"><span class="dk-k">Mount</span><span class="dk-v mono" title="${escapeHtml(mp)}">${escapeHtml(mp||'—')}</span></div>
          </div>
          <div class="dk-actions">
            <button class="btn btn-xs btn-danger dv-rm" data-name="${escapeHtml(nm)}" title="Remove">🗑 Remove</button>
          </div>
        </div>`;
      });
      html+='</div>';
      body.innerHTML=html;
      body.querySelectorAll('.dv-rm').forEach(btn=>{btn.onclick=function(){
        const n=this.dataset.name;
        confirmDialog({title:'Remove Volume',messageHtml:'Remove volume <strong>'+escapeHtml(n)+'</strong>?',okText:'Remove',danger:true,loadingText:'Removing...',onConfirm:async()=>{
          const r=await api('/docker/volumes/'+encodeURIComponent(n)+'/remove',{method:'POST'});
          if(!r.success)throw new Error(r.message||'Remove failed');
          showToast('Removed','success');renderDockerVolumes(dc);
        }});
      };});
    }
    document.getElementById('dv-add').onclick=()=>renderDockerVolumeCreate(dc);
    document.getElementById('dv-prune').onclick=()=>{
      confirmDialog({
        title:'Prune Volumes',
        messageHtml:'Remove all <strong>unused</strong> volumes? This cannot be undone.',
        okText:'Prune',danger:true,loadingText:'Pruning...',
        onConfirm:async()=>{
          const r=await api('/docker/volumes/prune',{method:'POST',body:'{}'});
          if(!r.success)throw new Error(r.message||'Prune failed');
          const d=r.data||{};
          const freed=d.space_reclaimed!=null?formatSize(d.space_reclaimed):'0 B';
          showToast('Pruned '+(d.deleted||0)+' · freed '+freed,'success');
          renderDockerVolumes(dc);
        }
      });
    };
  }catch(e){if(e.message!=='Unauthorized')dc.innerHTML='<div class="error-msg">Error: '+e.message+'</div>'}
}
function renderDockerVolumeCreate(dc){
  const m=openModal(`
    <h3>Create Volume</h3>
    <div class="field"><label>Name</label><input type="text" id="dv-name" placeholder="my-volume"></div>
    <div class="field"><label>Driver</label><select id="dv-driver"><option value="local">local</option></select></div>
    <div id="dv-err" class="error-msg" style="display:none"></div>
    <div class="btn-row">
      <button class="btn btn-sm btn-ghost" id="dv-c-back">Cancel</button>
      <button class="btn btn-sm btn-success" id="dv-c-do">Create</button>
    </div>`,{maxWidth:'440px'});
  m.root.querySelector('#dv-c-back').addEventListener('click',m.close);
  m.root.querySelector('#dv-c-do').addEventListener('click',async function(){
    const nm=m.root.querySelector('#dv-name').value.trim();
    const drv=m.root.querySelector('#dv-driver').value;
    const err=m.root.querySelector('#dv-err');
    if(!nm){err.textContent='Name required';err.style.display='block';return}
    err.style.display='none';this.disabled=true;this.textContent='Creating...';
    try{
      const r=await api('/docker/volumes',{method:'POST',body:JSON.stringify({name:nm,driver:drv})});
      if(r.success){showToast('Created','success');m.close();renderDockerVolumes(dc)}
      else{err.textContent=r.message||'Failed';err.style.display='block';this.disabled=false;this.textContent='Create'}
    }catch(e){if(e.message==='Unauthorized'){m.close();return}err.textContent='Error: '+e.message;err.style.display='block';this.disabled=false;this.textContent='Create'}
  });
}
// ── System Info ──
function infoCard(k,v){
  return `<div class="card info-item"><div class="info-k">${escapeHtml(k)}</div><div class="info-v">${escapeHtml(String(v))}</div></div>`;
}
async function renderSystemInfo(){
  const c=document.getElementById('content');
  if(!c)return;
  c.innerHTML='<div style="text-align:center;padding:2rem;color:var(--text-dim)">Loading system information...</div>';
  try{
    const j=await api('/os_info');
    if(!j.success||!j.data){c.innerHTML='<div class="error-msg">Failed to load system info</div>';return}
    const d=j.data;
    const os=d.os||{};
    const mem=d.memory||{};
    const load=d.load||{};
    const coreCount=Array.isArray(d.cpu)?d.cpu.length:0;
    const host=os.host_name||os.hostname||'N/A';
    const kernel=os.kernel_version||'N/A';
    const arch=os.architecture||'N/A';
    const osName=(os.name||os.os_type||'Linux')+(os.os_version?(' '+os.os_version):'');
    const bootSec=Number(os.boot_time)||0;
    const nowSec=Number(d.updated_at)||Math.floor(Date.now()/1000);
    const uptime=bootSec>0?formatUptime(nowSec-bootSec):'N/A';
    const totalMem=mem.total?formatSize(mem.total*1048576):'N/A';
    const loadStr=(load.one!=null)?(load.one.toFixed(2)+' / '+(load.five||0).toFixed(2)+' / '+(load.fifteen||0).toFixed(2)):'N/A';
    const bootStr=bootSec>0?new Date(bootSec*1000).toLocaleString():'N/A';
    const updStr=d.updated_at?new Date(d.updated_at*1000).toLocaleString():'N/A';
    c.innerHTML=`
    <div class="card ds-overview mb-4">
      <div class="ov-item"><span class="ov-k">Host</span><span class="ov-v">${escapeHtml(host)}</span></div>
      <div class="ov-item"><span class="ov-k">OS</span><span class="ov-v">${escapeHtml(osName)}</span></div>
      <div class="ov-item"><span class="ov-k">Kernel</span><span class="ov-v">${escapeHtml(kernel)}</span></div>
      <div class="ov-item"><span class="ov-k">Arch</span><span class="ov-v">${escapeHtml(arch)}</span></div>
      <div class="ov-item"><span class="ov-k">Uptime</span><span class="ov-v">${escapeHtml(uptime)}</span></div>
    </div>
    <div class="info-grid">
      ${infoCard('OS Name',os.name||'N/A')}
      ${infoCard('OS Version',os.os_version||'N/A')}
      ${infoCard('Platform',os.long_os_version||os.os_type||'N/A')}
      ${infoCard('Kernel',kernel)}
      ${infoCard('Architecture',arch)}
      ${infoCard('Hostname',host)}
      ${infoCard('CPU Cores',coreCount||'N/A')}
      ${infoCard('Total Memory',totalMem)}
      ${infoCard('Load 1m / 5m / 15m',loadStr)}
      ${infoCard('Boot Time',bootStr)}
      ${infoCard('Uptime',uptime)}
      ${infoCard('Updated At',updStr)}
    </div>`;
  }catch(e){
    if(e.message!=='Unauthorized')c.innerHTML=`<div class="error-msg">Error: ${e.message}</div>`;
  }
}

// ── User Management ──
async function renderUserManagement(){
  const c=document.getElementById('content');
  if(!c)return;
  c.innerHTML='<div style="text-align:center;padding:2rem;color:var(--text-dim)">Loading users...</div>';
  try{
    const j=await api('/admin/users');
    if(!j.success){c.innerHTML='<div class="error-msg">Failed to load users</div>';return}
    const users=j.data||[];
    if(!Array.isArray(users)){c.innerHTML='<div class="error-msg">Invalid user data</div>';return}
    renderUserTable(c,users);
  }catch(e){
    if(e.message!=='Unauthorized')c.innerHTML=`<div class="error-msg">Error: ${e.message}</div>`;
  }
}

function renderUserTable(c,users){
  let html=`
  <div class="flex justify-between items-center mb-4">
    <h2 style="font-size:1.1rem;font-weight:600">User Management</h2>
    <button class="btn btn-sm" id="create-user-btn">+ Create User</button>
  </div>
  <div class="card"><div class="table-wrap"><table>
    <thead><tr><th>ID</th><th>Username</th><th>Authority</th><th>Status</th><th>Created At</th><th>Actions</th></tr></thead>
    <tbody>`;
  users.forEach(u=>{
    const status=u.status||'active';
    const statusClass=status==='active'?'running':status==='banned'?'exited':'';
    html+=`<tr>
      <td style="font-family:monospace;font-size:0.8rem">${u.id||''}</td>
      <td>${escapeHtml(u.username||'')}</td>
      <td>${escapeHtml(u.authority||'')}</td>
      <td><span class="status-badge ${statusClass}">${status}</span></td>
      <td style="color:var(--text-dim);font-size:0.8rem">${u.created_at?formatTime(u.created_at):'N/A'}</td>
      <td>
        <button class="btn btn-sm edit-user-btn" data-id="${u.id}">Edit</button>
        <button class="btn btn-sm btn-danger delete-user-btn" data-id="${u.id}" data-username="${escapeHtml(u.username||'')}">Delete</button>
        <button class="btn btn-sm btn-warning reset-pwd-btn" data-id="${u.id}">Reset Pwd</button>
      </td>
    </tr>`;
  });
  html+=`</tbody></table></div></div>`;
  c.innerHTML=html;

  document.getElementById('create-user-btn').addEventListener('click',()=>showUserModal(null,users,renderUserManagement));
  c.querySelectorAll('.edit-user-btn').forEach(btn=>{
    btn.addEventListener('click',()=>{
      const id=parseInt(btn.dataset.id);
      const user=users.find(u=>u.id===id);
      if(user)showUserModal(user,users,renderUserManagement);
    });
  });
  c.querySelectorAll('.delete-user-btn').forEach(btn=>{
    btn.addEventListener('click',()=>showDeleteConfirm(btn.dataset.id,btn.dataset.username,renderUserManagement));
  });
  c.querySelectorAll('.reset-pwd-btn').forEach(btn=>{
    btn.addEventListener('click',()=>resetPassword(btn.dataset.id));
  });
}

function showUserModal(user,users,refreshFn){
  const isEdit=!!user;
  const m=openModal(`
    <h3>${isEdit?'Edit User':'Create User'}</h3>
    <div class="field">
      <label>Username</label>
      <input type="text" id="modal-username" value="${isEdit?escapeHtml(user.username||''):''}" ${isEdit?'readonly':''}>
    </div>
    <div class="field">
      <label>${isEdit?'New Password (leave blank to keep)':'Password'}</label>
      <input type="password" id="modal-password" placeholder="${isEdit?'Leave blank to keep current':''}">
    </div>
    <div class="field">
      <label>Authority</label>
      <select id="modal-authority">
        <option value="admin" ${isEdit&&user.authority==='admin'?'selected':''}>admin</option>
        <option value="user" ${isEdit&&user.authority==='user'?'selected':''}>user</option>
        <option value="view" ${isEdit&&user.authority==='view'?'selected':''}>view</option>
      </select>
    </div>
    <div id="modal-error" class="error-msg" style="display:none"></div>
    <div class="btn-row">
      <button class="btn btn-sm btn-ghost" id="modal-cancel-btn">Cancel</button>
      <button class="btn btn-sm btn-success" id="modal-save-btn">${isEdit?'Save':'Create'}</button>
    </div>`);
  const close=m.close;
  m.root.querySelector('#modal-cancel-btn').addEventListener('click',close);

  m.root.querySelector('#modal-save-btn').addEventListener('click',async()=>{
    const username=m.root.querySelector('#modal-username').value.trim();
    const password=m.root.querySelector('#modal-password').value;
    const authority=m.root.querySelector('#modal-authority').value;
    const errEl=m.root.querySelector('#modal-error');
    if(!username){errEl.textContent='Username is required';errEl.style.display='block';return}
    if(!isEdit&&!password){errEl.textContent='Password is required';errEl.style.display='block';return}
    errEl.style.display='none';
    const btn=m.root.querySelector('#modal-save-btn');
    btn.disabled=true;btn.textContent='Saving...';
    try{
      const body={username,password,authority};
      if(isEdit&&!password)delete body.password;
      const r=await api('/admin/users'+(isEdit?'/'+user.id:''),{
        method:isEdit?'PUT':'POST',
        body:JSON.stringify(body)
      });
      if(r.success){
        showToast(isEdit?'User updated':'User created','success');
        close();
        if(refreshFn)refreshFn(users);
      } else {
        errEl.textContent=r.message||'Operation failed';
        errEl.style.display='block';
        btn.disabled=false;btn.textContent=isEdit?'Save':'Create';
      }
    }catch(e){
      if(e.message!=='Unauthorized'){
        errEl.textContent='Error: '+e.message;
        errEl.style.display='block';
        btn.disabled=false;btn.textContent=isEdit?'Save':'Create';
      }
    }
  });
}

function showDeleteConfirm(id,username,refreshFn){
  confirmDialog({
    title:'Confirm Delete',
    messageHtml:`Are you sure you want to delete user <strong>${escapeHtml(username)}</strong>?`,
    okText:'Delete',danger:true,loadingText:'Deleting...',maxWidth:'380px',
    onConfirm:async()=>{
      const r=await api('/admin/users/'+id,{method:'DELETE'});
      if(!r.success)throw new Error(r.message||'Delete failed');
      showToast('User deleted','success');
      if(refreshFn)refreshFn();
    }
  });
}

async function resetPassword(id){
  const m=openModal(`
    <h3>Reset Password</h3>
    <p style="color:var(--text-muted);font-size:0.9rem;margin-bottom:1rem">Generate a new password for this user?</p>
    <div id="modal-error" class="error-msg" style="display:none"></div>
    <div class="btn-row">
      <button class="btn btn-sm btn-ghost" id="modal-cancel-btn">Cancel</button>
      <button class="btn btn-sm btn-warning" id="modal-reset-btn">Reset</button>
    </div>`,{maxWidth:'380px'});
  const close=m.close;
  m.root.querySelector('#modal-cancel-btn').addEventListener('click',close);

  m.root.querySelector('#modal-reset-btn').addEventListener('click',async()=>{
    const btn=m.root.querySelector('#modal-reset-btn');
    btn.disabled=true;btn.textContent='Resetting...';
    try{
      const r=await api('/admin/users/'+id+'/reset-password',{method:'POST'});
      if(r.success&&r.data){
        const newPwd=r.data.password||'N/A';
        m.root.innerHTML=`
          <h3>Password Reset</h3>
          <p style="color:var(--text-muted);font-size:0.9rem;margin-bottom:.5rem">New password for this user:</p>
          <div style="background:var(--bg);border:1px solid var(--border);border-radius:6px;padding:10px 14px;font-family:monospace;font-size:0.9rem;text-align:center;color:var(--primary);margin-bottom:1rem;user-select:all">${escapeHtml(newPwd)}</div>
          <p style="color:var(--text-dim);font-size:0.8rem">Please save this password. It cannot be retrieved later.</p>
          <div class="btn-row">
            <button class="btn btn-sm" id="modal-close-btn">Close</button>
          </div>`;
        m.root.querySelector('#modal-close-btn').addEventListener('click',close);
      } else {
        const errEl=m.root.querySelector('#modal-error');
        errEl.textContent=r.message||'Reset failed';
        errEl.style.display='block';
        btn.disabled=false;btn.textContent='Reset';
      }
    }catch(e){
      if(e.message!=='Unauthorized'){
        const errEl=m.root.querySelector('#modal-error');
        errEl.textContent='Error: '+e.message;
        errEl.style.display='block';
        btn.disabled=false;btn.textContent='Reset';
      }
    }
  });
}

// ── Process Management ──
async function renderProcessManagement(){
  const c=document.getElementById('content');
  if(!c)return;
  c.innerHTML=`
  <div class="flex justify-between items-center mb-4">
    <h2 style="font-size:1.1rem;font-weight:600">Process Management</h2>
    <span class="text-sm text-muted" id="process-count"></span>
  </div>
  <div class="dk-toolbar" style="margin-bottom:.85rem">
    <div class="dk-toolbar-left">
      <input type="search" class="dk-search" id="process-search-input" placeholder="Search name or PID…" value="${escapeHtml(processSearchKeyword||'')}" autocomplete="off">
      <button class="btn btn-sm btn-ghost" id="process-search-btn">Search</button>
    </div>
  </div>
  <div class="card" id="process-list-container">
    <div style="text-align:center;padding:2rem;color:var(--text-dim)">Loading processes...</div>
  </div>`;

  let debounce=null;
  const applySearch=()=>{
    processSearchKeyword=document.getElementById('process-search-input').value.trim();
    loadProcessList();
  };
  document.getElementById('process-search-btn').addEventListener('click',applySearch);
  document.getElementById('process-search-input').addEventListener('keydown',e=>{
    if(e.key==='Enter')applySearch();
  });
  document.getElementById('process-search-input').addEventListener('input',()=>{
    clearTimeout(debounce);
    debounce=setTimeout(applySearch,350);
  });

  await loadProcessList();
  processInterval=setInterval(loadProcessList,5000);
}

let processSort={key:'cpu',dir:'desc'};
function procSortIndicator(key){
  if(processSort.key!==key)return '';
  return processSort.dir==='asc'?' ▲':' ▼';
}
async function loadProcessList(){
  const container=document.getElementById('process-list-container');
  if(!container)return;
  try{
    let url='/process/list';
    if(processSearchKeyword)url+='?keyword='+encodeURIComponent(processSearchKeyword);
    const j=await api(url);
    if(!j.success){container.innerHTML='<div class="error-msg">Failed to load processes</div>';return}
    const processes=j.data||[];
    if(!Array.isArray(processes)){container.innerHTML='<div class="error-msg">Invalid process data</div>';return}
    // Client-side sort (default: CPU desc)
    const sd=processSort.dir==='asc'?1:-1,sk=processSort.key;
    processes.sort((a,b)=>{
      if(sk==='name'||sk==='status'){
        const av=(a[sk]||'').toLowerCase(),bv=(b[sk]||'').toLowerCase();
        return av<bv?-sd:av>bv?sd:0;
      }
      return ((+a[sk]||0)-(+b[sk]||0))*sd;
    });
    const cols=[['pid','PID'],['name','Name'],['cpu','CPU%'],['memory','Memory'],['status','Status']];
    let html='<div class="table-wrap"><table><thead><tr>'+
      cols.map(c=>`<th class="sortable" data-sort="${c[0]}">${c[1]}${procSortIndicator(c[0])}</th>`).join('')+
      '<th>Run Time</th><th>Actions</th></tr></thead><tbody>';
    const countEl=document.getElementById('process-count');
    if(countEl)countEl.textContent=processes.length+' process(es)';
    processes.forEach(p=>{
      const memStr=p.memory!=null?formatSize(p.memory):'N/A';
      const cpuStr=p.cpu!=null?(Number(p.cpu).toFixed(1)+'%'):'N/A';
      const rt=formatUptimeSecs(p.run_time!=null?p.run_time:p.runtime);
      html+=`<tr>
        <td style="font-family:monospace">${p.pid||''}</td>
        <td>${escapeHtml(p.name||'')}</td>
        <td>${cpuStr}</td>
        <td>${memStr}</td>
        <td>${escapeHtml(p.status||'')}</td>
        <td style="color:var(--text-dim);font-size:0.8rem">${escapeHtml(rt)}</td>
        <td><button class="btn btn-sm btn-danger kill-process-btn" data-pid="${p.pid}" data-name="${escapeHtml(p.name||'')}">Kill</button></td>
      </tr>`;
    });
    html+='</tbody></table></div>';
    container.innerHTML=html;
    container.querySelectorAll('th.sortable[data-sort]').forEach(th=>{
      th.addEventListener('click',()=>{
        const k=th.dataset.sort;
        if(processSort.key===k)processSort.dir=processSort.dir==='asc'?'desc':'asc';
        else{processSort.key=k;processSort.dir=(k==='name'||k==='status')?'asc':'desc'}
        loadProcessList();
      });
    });
    container.querySelectorAll('.kill-process-btn').forEach(btn=>{
      btn.addEventListener('click',function(){
        const pid=this.dataset.pid;
        const name=this.dataset.name;
        killProcess(pid,name);
      });
    });
  }catch(e){
    if(e.message!=='Unauthorized')container.innerHTML=`<div class="error-msg">Error: ${e.message}</div>`;
  }
}

function killProcess(pid,name){
  confirmDialog({
    title:'Kill Process',
    messageHtml:`Kill process PID ${escapeHtml(String(pid))} (<strong>${escapeHtml(name)}</strong>)?`,
    okText:'Kill',danger:true,loadingText:'Killing...',maxWidth:'380px',
    onConfirm:async()=>{
      const r=await api('/process/kill/'+encodeURIComponent(pid),{method:'POST'});
      if(r.success){
        showToast('Process killed','success');
        loadProcessList();
      } else {
        showToast(r.message||'Kill failed','error');
      }
    }
  });
}

// ── Service Management ──
async function renderServiceManagement(){
  const c=document.getElementById('content');
  if(!c)return;
  c.innerHTML='<div style="text-align:center;padding:2rem;color:var(--text-dim)">Loading services...</div>';
  try{
    const j=await api('/service/list');
    if(!j.success){c.innerHTML='<div class="error-msg">Failed to load services</div>';return}
    const services=j.data||[];
    if(!Array.isArray(services)){c.innerHTML='<div class="error-msg">Invalid service data</div>';return}
    let html=`
    <div class="flex justify-between items-center mb-4">
      <h2 style="font-size:1.1rem;font-weight:600">Service Management</h2>
    </div>
    <div class="card"><div class="table-wrap"><table>
      <thead><tr><th>Name</th><th>Load</th><th>Active</th><th>Sub</th><th>Description</th><th>Actions</th></tr></thead>
      <tbody>`;
    services.forEach(s=>{
      const active=s.active||'';
      const activeClass=active==='active'?'running':active==='inactive'?'exited':'';
      html+=`<tr class="service-row" data-name="${escapeHtml(s.name||'')}" style="cursor:pointer">
        <td>${escapeHtml(s.name||'')}</td>
        <td>${escapeHtml(s.load||'')}</td>
        <td><span class="status-badge ${activeClass}">${active}</span></td>
        <td>${escapeHtml(s.sub||'')}</td>
        <td style="color:var(--text-muted);max-width:300px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${escapeHtml(s.description||'')}</td>
        <td>
          <button class="btn btn-sm btn-success service-action" data-name="${escapeHtml(s.name||'')}" data-action="start">Start</button>
          <button class="btn btn-sm btn-warning service-action" data-name="${escapeHtml(s.name||'')}" data-action="stop">Stop</button>
          <button class="btn btn-sm service-action" data-name="${escapeHtml(s.name||'')}" data-action="restart">Restart</button>
        </td>
      </tr>`;
    });
    html+=`</tbody></table></div></div>`;
    c.innerHTML=html;

    c.querySelectorAll('.service-action').forEach(btn=>{
      btn.addEventListener('click',async function(e){
        e.stopPropagation();
        const name=this.dataset.name;
        const action=this.dataset.action;
        this.disabled=true;
        const orig=this.textContent;
        this.textContent='...';
        try{
        const r=await api('/service/'+encodeURIComponent(name)+'/'+action,{
          method:'POST'
        });
          if(r.success){
            showToast('Service '+action+' successful','success');
            renderServiceManagement();
          } else {
            showToast(r.message||action+' failed','error');
            this.disabled=false;this.textContent=orig;
          }
        }catch(e){
          if(e.message!=='Unauthorized'){
            showToast('Error: '+e.message,'error');
            this.disabled=false;this.textContent=orig;
          }
        }
      });
    });

    c.querySelectorAll('.service-row').forEach(row=>{
      row.addEventListener('click',function(){
        const name=this.dataset.name;
        showServiceStatus(name);
      });
    });
  }catch(e){
    if(e.message!=='Unauthorized')c.innerHTML=`<div class="error-msg">Error: ${e.message}</div>`;
  }
}

async function showServiceStatus(name){
  const overlay=document.createElement('div');
  overlay.className='modal-overlay';
  overlay.innerHTML=`
  <div class="modal" style="max-width:600px">
    <h3>Service Status: ${escapeHtml(name)}</h3>
    <pre style="background:var(--bg);border:1px solid var(--border);border-radius:6px;padding:12px;font-size:0.8rem;font-family:monospace;color:var(--text);max-height:300px;overflow:auto;white-space:pre-wrap;word-break:break-all" id="service-status-output">Loading...</pre>
    <div class="btn-row">
      <button class="btn btn-sm" id="modal-close-btn">Close</button>
    </div>
  </div>`;
  document.body.appendChild(overlay);

  function close(){overlay.remove()}
  document.getElementById('modal-close-btn').addEventListener('click',close);
  overlay.addEventListener('click',e=>{if(e.target===overlay)close()});

  try{
    const r=await api('/service/'+encodeURIComponent(name)+'/status',{
      method:'GET'
    });
    const outputEl=document.getElementById('service-status-output');
    if(r.success&&r.data){
      outputEl.textContent=typeof r.data==='string'?r.data:JSON.stringify(r.data,null,2);
    } else {
      outputEl.textContent=r.message||'Failed to get status';
    }
  }catch(e){
    if(e.message!=='Unauthorized'){
      document.getElementById('service-status-output').textContent='Error: '+e.message;
    }
  }
}

// ── Log Viewer ──
async function renderLogViewer(){
  const c=document.getElementById('content');
  if(!c)return;
  currentLogKeyword='';
  currentLogFile='';
  c.innerHTML=`
  <div class="flex justify-between items-center mb-4">
    <h2 style="font-size:1.1rem;font-weight:600">Log Viewer</h2>
    <div class="flex items-center gap-3">
      <span id="log-refresh-status" class="text-sm text-muted"></span>
      <label class="toggle-switch">
        <input type="checkbox" id="log-auto-refresh">
        <span class="toggle-slider"></span>
      </label>
      <span class="text-sm text-muted">Auto-refresh</span>
    </div>
  </div>
  <div class="log-layout">
    <div class="log-sidebar card">
      <div style="font-size:0.85rem;color:var(--text-dim);margin-bottom:.5rem;padding:0 4px">Log Files</div>
      <div id="log-file-list" style="max-height:450px;overflow-y:auto">
        <div style="text-align:center;padding:1rem;color:var(--text-dim);font-size:0.85rem">Loading...</div>
      </div>
    </div>
    <div class="log-content">
      <div class="log-toolbar">
        <input type="text" id="log-keyword-input" placeholder="Search in log..." style="max-width:250px">
        <button class="btn btn-sm" id="log-search-btn">Search</button>
        <button class="btn btn-sm" id="log-clear-btn">Clear</button>
        <span id="log-file-name" style="flex:1;font-size:0.85rem;color:var(--text-muted);text-align:right;overflow:hidden;text-overflow:ellipsis;white-space:nowrap"></span>
      </div>
      <pre class="log-text" id="log-content-area">Select a log file</pre>
    </div>
  </div>`;

  document.getElementById('log-search-btn').addEventListener('click',()=>{
    currentLogKeyword=document.getElementById('log-keyword-input').value.trim();
    if(currentLogFile)loadLogContent(currentLogFile);
  });
  document.getElementById('log-keyword-input').addEventListener('keydown',e=>{
    if(e.key==='Enter')document.getElementById('log-search-btn').click();
  });
  document.getElementById('log-clear-btn').addEventListener('click',()=>{
    document.getElementById('log-keyword-input').value='';
    currentLogKeyword='';
    if(currentLogFile)loadLogContent(currentLogFile);
  });
  document.getElementById('log-auto-refresh').addEventListener('change',function(){
    logRefreshEnabled=this.checked;
    if(logRefreshEnabled&&currentLogFile){
      if(logRefreshInterval)clearInterval(logRefreshInterval);
      logRefreshInterval=setInterval(()=>loadLogContent(currentLogFile,{silent:true}),5000);
    } else {
      if(logRefreshInterval)clearInterval(logRefreshInterval);
      logRefreshInterval=null;
      const st=document.getElementById('log-refresh-status');
      if(st)st.textContent='';
    }
  });

  await loadLogFileList();
}

async function loadLogFileList(){
  const listEl=document.getElementById('log-file-list');
  if(!listEl)return;
  try{
    const j=await api('/log/list');
    if(!j.success){listEl.innerHTML='<div style="color:var(--error);padding:1rem;font-size:0.85rem">Failed to load</div>';return}
    const files=j.data||[];
    if(!Array.isArray(files)){listEl.innerHTML='<div style="color:var(--text-dim);padding:1rem;font-size:0.85rem">No log files</div>';return}
    let html='';
    files.forEach(f=>{
      const fileName=typeof f==='string'?f:(f.path||f.name||'');
      const displayName=fileName.split('/').pop()||fileName;
      html+=`<div class="log-file-item${currentLogFile===fileName?' active':''}" data-path="${escapeHtml(fileName)}">${escapeHtml(displayName)}</div>`;
    });
    listEl.innerHTML=html||'<div style="color:var(--text-dim);padding:1rem;font-size:0.85rem;text-align:center">No log files</div>';
    listEl.querySelectorAll('.log-file-item').forEach(el=>{
      el.addEventListener('click',function(){
        currentLogFile=this.dataset.path;
        document.getElementById('log-file-list').querySelectorAll('.log-file-item').forEach(i=>i.classList.remove('active'));
        this.classList.add('active');
        document.getElementById('log-file-name').textContent=currentLogFile;
        loadLogContent(currentLogFile);
      });
    });
  }catch(e){
    if(e.message!=='Unauthorized')listEl.innerHTML='<div style="color:var(--error);padding:1rem;font-size:0.85rem">Error: '+e.message+'</div>';
  }
}

async function loadLogContent(filePath,opts={}){
  const area=document.getElementById('log-content-area');
  if(!area)return;
  const silent=!!opts.silent;
  const statusEl=document.getElementById('log-refresh-status');
  const nearBottom=(area.scrollHeight-area.scrollTop-area.clientHeight)<48;
  const prevScroll=area.scrollTop;
  const hadContent=area.textContent&&area.textContent!=='Loading...'&&area.textContent!=='Select a log file';
  if(!silent||!hadContent)area.textContent='Loading...';
  else if(statusEl)statusEl.textContent='Refreshing…';
  try{
    let url='/log/read?path='+encodeURIComponent(filePath)+'&lines=200';
    if(currentLogKeyword)url+='&keyword='+encodeURIComponent(currentLogKeyword);
    const j=await api(url);
    if(!j.success){area.textContent=j.message||'Failed to load log';if(statusEl)statusEl.textContent='';return}
    const lines=j.data&&Array.isArray(j.data.lines)?j.data.lines:(typeof j.data==='string'?[j.data]:[]);
    const text=lines.join('\n');
    if(currentLogKeyword){
      const regex=new RegExp(currentLogKeyword.replace(/[.*+?^${}()|[\]\\]/g,'\\$&'),'gi');
      area.innerHTML=text.split('\n').map(line=>{
        const highlighted=line.replace(regex,m=>`<span style="background:rgba(245,158,11,.3);color:var(--warning);border-radius:2px;padding:0 2px">${m}</span>`);
        return highlighted;
      }).join('\n');
    } else {
      area.textContent=text;
    }
    // Stick to bottom when following live logs; otherwise preserve scroll position.
    if(!silent||nearBottom||!hadContent)area.scrollTop=area.scrollHeight;
    else area.scrollTop=prevScroll;
    if(statusEl){
      const t=new Date();
      statusEl.textContent='Updated '+t.toLocaleTimeString();
    }
  }catch(e){
    if(e.message!=='Unauthorized')area.textContent='Error: '+e.message;
    if(statusEl)statusEl.textContent='';
  }
}

// ── Real-Time Monitor ──
let monitorCharts=null;
async function renderMonitor(){
  const c=document.getElementById('content');
  if(!c)return;

  c.innerHTML=`
  <div class="flex justify-between items-center mb-4">
    <h2 style="font-size:1.1rem;font-weight:600">Real-Time Monitor</h2>
    <span id="monitor-status" class="text-sm" style="color:var(--text-dim)">● Connecting…</span>
  </div>
  <div class="monitor-grid mb-4">
    <div class="card monitor-card">
      <div class="label">CPU Usage</div>
      <div class="value" id="monitor-cpu" style="color:var(--success)">0%</div>
      <div class="progress-bar mt-2"><div class="fill green" id="monitor-cpu-bar" style="width:0%"></div></div>
    </div>
    <div class="card monitor-card">
      <div class="label">Memory</div>
      <div class="value" id="monitor-memory" style="color:var(--primary)">0 / 0</div>
      <div class="progress-bar mt-2"><div class="fill blue" id="monitor-memory-bar" style="width:0%"></div></div>
    </div>
    <div class="card monitor-card">
      <div class="label">Swap</div>
      <div class="value" id="monitor-swap" style="color:var(--secondary)">0 / 0</div>
      <div class="progress-bar mt-2"><div class="fill blue" id="monitor-swap-bar" style="width:0%"></div></div>
    </div>
    <div class="card monitor-card">
      <div class="label">Uptime</div>
      <div class="value" id="monitor-uptime" style="color:var(--text);font-size:1.25rem">—</div>
    </div>
  </div>
  <div class="card-grid card-grid-3">
    <div class="card"><h3 class="ds-h">CPU %</h3><div class="live-chart"><svg id="chart-cpu"></svg></div></div>
    <div class="card"><h3 class="ds-h">Memory %</h3><div class="live-chart"><svg id="chart-mem"></svg></div></div>
    <div class="card"><h3 class="ds-h">Load (1m)</h3><div class="live-chart"><svg id="chart-load"></svg></div></div>
  </div>
  <div class="card mt-4">
    <h3 class="ds-h">Network</h3>
    <div style="display:grid;grid-template-columns:1fr 1fr;gap:1rem">
      <div>
        <div class="text-sm text-muted mb-2">Received</div>
        <div class="value" id="monitor-net-rx" style="font-size:1.5rem;font-weight:700;color:var(--primary)">0 B/s</div>
      </div>
      <div>
        <div class="text-sm text-muted mb-2">Transmitted</div>
        <div class="value" id="monitor-net-tx" style="font-size:1.5rem;font-weight:700;color:var(--secondary)">0 B/s</div>
      </div>
    </div>
  </div>`;

  // Uptime needs the boot time, which the SSE payload doesn't carry.
  let bootTime=0;
  try{const oj=await api('/os_info');if(oj&&oj.success&&oj.data&&oj.data.os)bootTime=Number(oj.data.os.boot_time)||0}catch(e){}

  monitorCharts={
    bootTime,
    cpu:createLiveChart(document.getElementById('chart-cpu'),{max:100,series:[{key:'v',color:'var(--primary)'}]}),
    mem:createLiveChart(document.getElementById('chart-mem'),{max:100,series:[{key:'v',color:'var(--secondary)'}]}),
    load:createLiveChart(document.getElementById('chart-load'),{series:[{key:'v',color:'var(--warning)'}]})
  };

  try{
    const token=getToken();
    const url=apiUrl('/monitor')+'?token='+encodeURIComponent(token||'');
    monitorEventSource=new EventSource(url);

    monitorEventSource.onmessage=function(e){
      const statusEl=document.getElementById('monitor-status');
      if(statusEl){statusEl.textContent='● Connected';statusEl.style.color='var(--success)'}
      try{
        const data=JSON.parse(e.data);
        updateMonitor(data);
      }catch(parseErr){}
    };

    monitorEventSource.onerror=function(){
      const statusEl=document.getElementById('monitor-status');
      if(statusEl){statusEl.textContent='✗ Disconnected';statusEl.style.color='var(--error)'}
    };
  }catch(e){
    const statusEl=document.getElementById('monitor-status');
    if(statusEl){statusEl.textContent='✗ Connection failed';statusEl.style.color='var(--error)'}
  }
}

function updateMonitor(data){
  const cpu=data.cpu!=null?parseFloat(data.cpu):null;
  const memUsed=data.memory&&data.memory.used!=null?data.memory.used:null;
  const memTotal=data.memory&&data.memory.total!=null?data.memory.total:null;
  const swapUsed=data.swap&&data.swap.used!=null?data.swap.used:null;
  const swapTotal=data.swap&&data.swap.total!=null?data.swap.total:null;
  // Aggregate network rates across all interfaces
  let netRx=null, netTx=null;
  if(data.network&&Array.isArray(data.network)){
    netRx=data.network.reduce((s,n)=>s+(n.received||0),0);
    netTx=data.network.reduce((s,n)=>s+(n.transmitted||0),0);
  }

  if(cpu!==null){
    const cpuEl=document.getElementById('monitor-cpu');
    const cpuBar=document.getElementById('monitor-cpu-bar');
    if(cpuEl){cpuEl.textContent=cpu.toFixed(1)+'%';cpuEl.style.color=cpu>80?'var(--error)':cpu>50?'var(--warning)':'var(--success)'}
    if(cpuBar){cpuBar.style.width=Math.min(cpu,100)+'%';cpuBar.className='fill '+(cpu>80?'red':cpu>50?'orange':'green')}
  }

  if(memUsed!==null&&memTotal!==null&&memTotal>0){
    const pct=(memUsed/memTotal)*100;
    const memEl=document.getElementById('monitor-memory');
    const memBar=document.getElementById('monitor-memory-bar');
    if(memEl)memEl.textContent=formatSize(memUsed)+' / '+formatSize(memTotal);
    if(memBar){memBar.style.width=Math.min(pct,100)+'%';memBar.className='fill '+(pct>80?'red':pct>50?'orange':'blue')}
  }

  if(swapUsed!==null&&swapTotal!==null&&swapTotal>0){
    const pct=(swapUsed/swapTotal)*100;
    const swapEl=document.getElementById('monitor-swap');
    const swapBar=document.getElementById('monitor-swap-bar');
    if(swapEl)swapEl.textContent=formatSize(swapUsed)+' / '+formatSize(swapTotal);
    if(swapBar){swapBar.style.width=Math.min(pct,100)+'%';swapBar.className='fill '+(pct>80?'red':pct>50?'orange':'blue')}
  } else if(swapUsed!==null&&swapTotal!==null){
    const swapEl=document.getElementById('monitor-swap');
    if(swapEl)swapEl.textContent=formatSize(swapUsed)+' / '+formatSize(swapTotal);
  }

  if(netRx!==null){
    const rxEl=document.getElementById('monitor-net-rx');
    if(rxEl)rxEl.textContent=formatSize(netRx)+'/s';
  }
  if(netTx!==null){
    const txEl=document.getElementById('monitor-net-tx');
    if(txEl)txEl.textContent=formatSize(netTx)+'/s';
  }

  // Uptime (from boot time captured on page load)
  if(monitorCharts&&monitorCharts.bootTime>0){
    const upEl=document.getElementById('monitor-uptime');
    if(upEl)upEl.textContent=formatUptime(Math.floor(Date.now()/1000)-monitorCharts.bootTime);
  }

  // Push rolling history into the live charts
  if(monitorCharts){
    if(cpu!==null)monitorCharts.cpu.push({v:cpu});
    if(memUsed!==null&&memTotal){monitorCharts.mem.push({v:(memUsed/memTotal)*100})}
    const load1=data.load&&data.load.one!=null?data.load.one:null;
    if(load1!==null)monitorCharts.load.push({v:load1});
  }
}

// ── Web Server ──
let webServerTab='status';

async function renderWebServer(){
  const c=document.getElementById('content');
  if(!c)return;
  webServerTab='status';
  c.innerHTML=`
  <div class="flex justify-between items-center mb-4">
    <h2 style="font-size:1.1rem;font-weight:600">Web Server</h2>
  </div>
  <div class="card mb-4" id="webserver-status-card">
    <div style="text-align:center;padding:1rem;color:var(--text-dim)">Loading status...</div>
  </div>
  <div class="tabs">
    <div class="tab active" data-tab="status">Sites List</div>
    <div class="tab" data-tab="create">Create Site</div>
    <div class="tab" data-tab="ssl">SSL</div>
    <div class="tab" data-tab="config">Config</div>
  </div>
  <div id="webserver-tab-content">
    <div style="text-align:center;padding:2rem;color:var(--text-dim)">Select a tab</div>
  </div>`;

  c.querySelectorAll('.tab').forEach(tab=>{
    tab.addEventListener('click',function(){
      webServerTab=this.dataset.tab;
      c.querySelectorAll('.tab').forEach(t=>t.classList.remove('active'));
      this.classList.add('active');
      renderWebServerTab(webServerTab);
    });
  });

  await loadWebServerStatus();
  renderWebServerTab(webServerTab);
}

async function loadWebServerStatus(){
  const card=document.getElementById('webserver-status-card');
  if(!card)return;
  try{
    const j=await api('/webserver/status');
    if(!j.success){card.innerHTML='<div class="error-msg">Failed to load status</div>';return}
    const d=j.data||{};
    const installed=d.installed?'Yes':'No';
    const running=d.running?'Yes':'No';
    const version=d.version||'';
    const configTest=d.config_test||d.config_test_result||'N/A';
    card.innerHTML=`
    <div style="display:grid;grid-template-columns:repeat(4,1fr);gap:1rem">
      <div><div class="text-sm text-dim">Installed</div><div style="font-size:1.25rem;font-weight:600;color:${d.installed?'var(--success)':'var(--error)'}">${installed}</div></div>
      <div><div class="text-sm text-dim">Running</div><div style="font-size:1.25rem;font-weight:600;color:${d.running?'var(--success)':'var(--error)'}">${running}</div></div>
      <div><div class="text-sm text-dim">Version</div><div style="font-size:1rem;font-weight:600;color:var(--text)">${version?escapeHtml(version):'N/A'}</div></div>
      <div><div class="text-sm text-dim">Config Test</div><div style="font-size:1rem;font-weight:600;color:${(configTest+'').toLowerCase()==='successful'||(configTest+'').toLowerCase()==='ok'?'var(--success)':'var(--warning)'}">${escapeHtml(configTest)}</div></div>
    </div>`;
  }catch(e){
    if(e.message!=='Unauthorized')card.innerHTML='<div class="error-msg">Error: '+e.message+'</div>';
  }
}

async function renderWebServerTab(tab){
  const tc=document.getElementById('webserver-tab-content');
  if(!tc)return;
  if(tab==='status')await renderWebServerSites(tc);
  else if(tab==='create')renderWebServerCreate(tc);
  else if(tab==='ssl')renderWebServerSsl(tc);
  else if(tab==='config')renderWebServerConfig(tc);
}

async function renderWebServerSites(tc){
  tc.innerHTML='<div style="text-align:center;padding:2rem;color:var(--text-dim)">Loading sites...</div>';
  try{
    const j=await api('/webserver/sites');
    if(!j.success){tc.innerHTML='<div class="error-msg">Failed to load sites</div>';return}
    const sites=j.data||[];
    if(!Array.isArray(sites)){tc.innerHTML='<div class="error-msg">Invalid site data</div>';return}
    let html=`<div class="dk-toolbar">
      <div class="dk-toolbar-left"><span class="text-sm text-muted">${sites.length} site(s)</span></div>
      <div class="dk-toolbar-right">
        <button class="btn btn-sm btn-ghost" id="ws-sites-reload">Reload Nginx</button>
        <button class="btn btn-sm btn-ghost" id="ws-sites-refresh">⟳ Refresh</button>
      </div>
    </div>`;
    if(sites.length===0){
      html+='<div class="empty-state"><div class="icon">🌐</div><div>No sites yet — create one from the Create Site tab</div></div>';
      tc.innerHTML=html;
    } else {
      html+='<div class="dk-grid">';
      sites.forEach(s=>{
        const displayName=s.server_name||s.name||'';
        const fileName=s.name||'';
        const listen=String(s.listen||'80');
        const proxy=s.proxy_pass||'';
        const root=s.root||s.document_root||'';
        const target=proxy||root||'—';
        const enabled=s.enabled!=null?s.enabled:true;
        const ssl=!!s.ssl;
        const path=s.path||'';
        html+=`<div class="dk-card">
          <div class="dk-card-head">
            <div class="dk-name" title="${escapeHtml(displayName)}">${escapeHtml(displayName)}</div>
            <span class="status-badge ${enabled?'running':'exited'}">${enabled?'Enabled':'Disabled'}</span>
          </div>
          <div class="dk-meta">
            <div class="dk-row"><span class="dk-k">Listen</span><span class="dk-v mono">${escapeHtml(listen)}</span></div>
            <div class="dk-row"><span class="dk-k">${proxy?'Proxy':'Root'}</span><span class="dk-v" title="${escapeHtml(target)}">${escapeHtml(target)}</span></div>
            <div class="dk-row"><span class="dk-k">SSL</span><span class="dk-v">${ssl?'Yes':'No'}</span></div>
            <div class="dk-row"><span class="dk-k">File</span><span class="dk-v mono" title="${escapeHtml(path||fileName)}">${escapeHtml(fileName)}</span></div>
          </div>
          <div class="dk-actions">
            <button class="btn btn-xs ${enabled?'btn-warning':'btn-success'} site-toggle-btn" data-name="${escapeHtml(fileName)}" data-enable="${enabled?'false':'true'}">${enabled?'Disable':'Enable'}</button>
            <button class="btn btn-xs btn-danger site-delete-btn" data-name="${escapeHtml(fileName)}">Delete</button>
          </div>
        </div>`;
      });
      html+='</div>';
      tc.innerHTML=html;
    }
    document.getElementById('ws-sites-refresh').onclick=()=>renderWebServerSites(tc);
    document.getElementById('ws-sites-reload').onclick=async function(){
      this.disabled=true;const orig=this.textContent;this.textContent='Reloading…';
      try{
        const r=await api('/webserver/reload',{method:'POST'});
        if(r.success){showToast('Nginx reloaded','success');loadWebServerStatus()}
        else showToast(r.message||'Reload failed','error');
      }catch(e){if(e.message!=='Unauthorized')showToast('Error: '+e.message,'error')}
      this.disabled=false;this.textContent=orig;
    };
    tc.querySelectorAll('.site-toggle-btn').forEach(btn=>{
      btn.addEventListener('click',async function(){
        const name=this.dataset.name;
        const enable=this.dataset.enable==='true';
        const action=enable?'enable':'disable';
        this.disabled=true;this.textContent='…';
        try{
          const r=await api('/webserver/sites/'+encodeURIComponent(name)+'/'+action,{method:'POST'});
          if(r.success){showToast('Site '+(enable?'enabled':'disabled'),'success');renderWebServerSites(tc)}
          else showToast(r.message||'Toggle failed','error');
        }catch(e){if(e.message!=='Unauthorized')showToast('Error: '+e.message,'error')}
      });
    });
    tc.querySelectorAll('.site-delete-btn').forEach(btn=>{
      btn.addEventListener('click',function(){
        const name=this.dataset.name;
        confirmDialog({title:'Delete Site',messageHtml:'Delete site <strong>'+escapeHtml(name)+'</strong>?',okText:'Delete',danger:true,loadingText:'Deleting...',onConfirm:async()=>{
          const r=await api('/webserver/sites/'+encodeURIComponent(name),{method:'DELETE'});
          if(!r.success)throw new Error(r.message||'Delete failed');
          showToast('Site deleted','success');renderWebServerSites(tc);
        }});
      });
    });
  }catch(e){
    if(e.message!=='Unauthorized')tc.innerHTML='<div class="error-msg">Error: '+e.message+'</div>';
  }
}

function renderWebServerCreate(tc){
  tc.innerHTML=`
  <div class="card">
    <div class="form-row">
      <div class="field">
        <label>Server Name</label>
        <input type="text" id="ws-server-name" placeholder="example.com">
      </div>
      <div class="field">
        <label>Listen Port</label>
        <input type="number" id="ws-listen" min="1" max="65535" value="80" placeholder="80">
      </div>
    </div>
    <div class="form-row">
      <div class="field">
        <label>Root Directory</label>
        <input type="text" id="ws-root" placeholder="/var/www/example">
      </div>
      <div class="field">
        <label>Proxy Pass (optional)</label>
        <input type="text" id="ws-proxy-pass" placeholder="http://127.0.0.1:3000">
      </div>
    </div>
    <div class="form-row">
      <div class="field">
        <label>SSL</label>
        <select id="ws-ssl">
          <option value="">None</option>
          <option value="letsencrypt">Let's Encrypt</option>
          <option value="custom">Custom</option>
        </select>
      </div>
      <div class="field">
        <label>Index Files</label>
        <input type="text" id="ws-index" placeholder="index.html index.htm" value="index.html index.htm">
      </div>
    </div>
    <div class="field">
      <label>Custom Config (optional)</label>
      <textarea id="ws-custom-config" rows="6" class="config-area" placeholder="# Additional nginx directives"></textarea>
    </div>
    <div id="ws-create-error" class="error-msg" style="display:none"></div>
    <button class="btn btn-success mt-3" id="ws-create-btn">Create Site</button>
  </div>`;

  document.getElementById('ws-create-btn').addEventListener('click',async()=>{
    const name=document.getElementById('ws-server-name').value.trim();
    const listen=parseInt(document.getElementById('ws-listen').value,10)||80;
    const root=document.getElementById('ws-root').value.trim();
    const proxyPass=document.getElementById('ws-proxy-pass').value.trim();
    const ssl=document.getElementById('ws-ssl').value;
    const index=document.getElementById('ws-index').value.trim();
    const customConfig=document.getElementById('ws-custom-config').value.trim();
    const errEl=document.getElementById('ws-create-error');

    if(!name){errEl.textContent='Server name is required';errEl.style.display='block';return}
    if(!root&&!proxyPass){errEl.textContent='Root directory or proxy pass is required';errEl.style.display='block';return}
    errEl.style.display='none';

    const btn=document.getElementById('ws-create-btn');
    btn.disabled=true;btn.textContent='Creating...';
    try{
      const body={server_name:name,listen,root,proxy_pass:proxyPass,ssl,index,extra_config:customConfig};
      const r=await api('/webserver/sites',{method:'POST',body:JSON.stringify(body)});
      if(r.success){
        showToast('Site created','success');
        webServerTab='status';
        renderWebServer();
      } else {
        errEl.textContent=r.message||'Create failed';
        errEl.style.display='block';
        btn.disabled=false;btn.textContent='Create Site';
      }
    }catch(e){
      if(e.message!=='Unauthorized'){
        errEl.textContent='Error: '+e.message;
        errEl.style.display='block';
        btn.disabled=false;btn.textContent='Create Site';
      }
    }
  });
}

function renderWebServerConfig(tc){
  tc.innerHTML=`
  <div class="card">
    <p style="color:var(--text-muted);margin-bottom:1rem;font-size:0.9rem">Manage Nginx configuration</p>
    <button class="btn btn-warning" id="ws-reload-btn">Reload Nginx</button>
    <div id="ws-reload-result" class="mt-3" style="display:none"></div>
  </div>`;

  document.getElementById('ws-reload-btn').addEventListener('click',async()=>{
    const btn=document.getElementById('ws-reload-btn');
    btn.disabled=true;btn.textContent='Reloading...';
    const resultEl=document.getElementById('ws-reload-result');
    resultEl.style.display='none';
    try{
      const r=await api('/webserver/reload',{method:'POST'});
      resultEl.style.display='block';
      if(r.success){
        resultEl.innerHTML='<span style="color:var(--success)">✓ Nginx reloaded successfully</span>';
        loadWebServerStatus();
      } else {
        resultEl.innerHTML='<span style="color:var(--error)">✗ '+(r.message||'Reload failed')+'</span>';
      }
    }catch(e){
      if(e.message!=='Unauthorized'){
        resultEl.style.display='block';
        resultEl.innerHTML='<span style="color:var(--error)">✗ Error: '+e.message+'</span>';
      }
    }finally{
      btn.disabled=false;btn.textContent='Reload Nginx';
    }
  });
}

// ── SSL Certificates (1Panel-style) ──
async function renderWebServerSsl(tc){
  tc.innerHTML='<div style="padding:1rem;color:var(--text-dim);text-align:center">Loading...</div>';
  try{
    const [certJ, acctJ] = await Promise.all([
      api('/ssl/certificates'),
      api('/ssl/accounts')
    ]);

    const d=certJ.data||{};
    const certs=d.certificates||[];
    const installed=d.installed;
    const accounts=acctJ.data?.accounts||[];
    const hasAccount=accounts.length>0;

    if(!installed){
      tc.innerHTML=`
      <div class="card" style="text-align:center;padding:2rem">
        <p style="color:var(--text-dim);margin-bottom:1rem">certbot not installed.</p>
        <p style="font-size:0.85rem;color:var(--text-muted)">
          <code style="display:inline-block;padding:6px 12px;background:var(--bg);border:1px solid var(--border);border-radius:4px">sudo apt install certbot</code>
          <code style="display:inline-block;margin-top:4px;padding:6px 12px;background:var(--bg);border:1px solid var(--border);border-radius:4px">sudo dnf install certbot</code>
        </p>
      </div>`;
      return;
    }

    // Build layout: header + accounts info + cert table
    let html='';
    html+=`<div class="flex justify-between items-center mb-4">
      <h3 style="font-size:0.95rem;font-weight:600">SSL Certificates</h3>
      <div>
        <button class="btn btn-sm btn-success" id="ssl-issue-btn">+ Issue</button>
        <button class="btn btn-sm" id="ssl-upload-btn" style="margin-left:4px">Upload</button>
        <button class="btn btn-sm" id="ssl-selfsigned-btn" style="margin-left:4px">Self-Signed</button>
      </div>
    </div>`;

    // Account status bar
    if(hasAccount){
      html+=`<div class="card mb-4" style="padding:8px 14px;background:var(--bg);font-size:0.85rem">
        <span style="color:var(--success)">✓</span> ACME account: <strong>${escapeHtml(accounts[0].email||'')}</strong>
        ${accounts[0].ca?'<span style="color:var(--text-dim)"> | CA: '+escapeHtml(accounts[0].ca)+'</span>':''}
      </div>`;
    } else {
      html+=`<div class="card mb-4" style="padding:8px 14px;background:var(--bg);font-size:0.85rem">
        <span style="color:var(--warning)">⚠</span> No ACME account registered.
        <button class="btn btn-sm" id="ssl-register-account-btn" style="margin-left:8px">Register</button>
      </div>`;
    }

    // Certificate cards
    if(certs.length===0){
      html+='<div class="empty-state"><div class="icon">🔒</div><div>No certificates. Click "+ Issue" to create one.</div></div>';
    } else {
      html+='<div class="dk-grid">';
      certs.forEach(c=>{
        const domain=c.domain||'?';
        const expiry=c.expiry||'Unknown';
        const expDate=new Date(expiry);
        const valid=!isNaN(expDate.getTime());
        const msLeft=valid?(expDate-Date.now()):null;
        const expired=valid&&msLeft<0;
        const expiringSoon=valid&&msLeft>=0&&msLeft<30*24*60*60*1000;
        const daysLeft=valid?Math.ceil(msLeft/(24*60*60*1000)):null;
        const expStr=valid?expDate.toLocaleDateString():String(expiry);
        let badge='running', badgeText='Valid';
        if(expired){badge='exited';badgeText='Expired'}
        else if(expiringSoon){badge='paused';badgeText=daysLeft+'d left'}
        else if(valid){badgeText=daysLeft+'d left'}
        const sans=(c.san||[]).filter(s=>s!==c.domain).join(', ');
        const keyType=c.key_type||'';
        html+=`<div class="dk-card">
          <div class="dk-card-head">
            <div class="dk-name" title="${escapeHtml(domain)}">${escapeHtml(domain)}</div>
            <span class="status-badge ${badge}">${escapeHtml(badgeText)}</span>
          </div>
          <div class="dk-meta">
            <div class="dk-row"><span class="dk-k">Expiry</span><span class="dk-v">${escapeHtml(expStr)}</span></div>
            <div class="dk-row"><span class="dk-k">SAN</span><span class="dk-v" title="${escapeHtml(sans)}">${escapeHtml(sans||'—')}</span></div>
            <div class="dk-row"><span class="dk-k">Key</span><span class="dk-v">${escapeHtml(keyType||'—')}</span></div>
          </div>
          <div class="dk-actions">
            <button class="btn btn-xs btn-ghost ssl-renew-btn" data-domain="${escapeHtml(domain)}">Renew</button>
            <button class="btn btn-xs btn-ghost ssl-deploy-btn" data-domain="${escapeHtml(domain)}">Deploy</button>
            <button class="btn btn-xs btn-danger ssl-delete-btn" data-domain="${escapeHtml(domain)}">Delete</button>
          </div>
        </div>`;
      });
      html+='</div>';
    }

    tc.innerHTML=html;

    // Issue button
    document.getElementById('ssl-issue-btn')?.addEventListener('click',async()=>{
      const provJ=await api('/ssl/providers');
      renderWebServerSslIssue(tc,provJ.data?.providers||[]);
    });

    // Register account
    document.getElementById('ssl-register-account-btn')?.addEventListener('click',()=>renderWebServerSslAccount(tc));
    document.getElementById('ssl-upload-btn')?.addEventListener('click',()=>renderWebServerSslUpload(tc));
    document.getElementById('ssl-selfsigned-btn')?.addEventListener('click',()=>renderWebServerSslSelfSigned(tc));

    // Renew
    tc.querySelectorAll('.ssl-renew-btn').forEach(btn=>{
      btn.addEventListener('click',async function(){
        const domain=this.dataset.domain;
        this.disabled=true;this.textContent='...';
        try{
          const r=await api('/ssl/renew/'+encodeURIComponent(domain),{method:'POST'});
          if(r.success) showToast('Renewed: '+domain,'success');
          else showToast(r.message||'Renew failed','error');
        }catch(e){if(e.message!=='Unauthorized')showToast('Error: '+e.message,'error')}
        renderWebServerSsl(tc);
      });
    });

    // Deploy
    tc.querySelectorAll('.ssl-deploy-btn').forEach(btn=>{
      btn.addEventListener('click',async function(){
        const domain=this.dataset.domain;
        let sites=[];
        try{
          const sj=await api('/webserver/sites');
          sites=(sj.data||[]).map(s=>s.name||s.filename||s).filter(Boolean);
        }catch(e){}
        const optsHtml=sites.length
          ? sites.map(n=>`<option value="${escapeHtml(n)}"${n===domain||n===domain+'.conf'?' selected':''}>${escapeHtml(n)}</option>`).join('')
          : '';
        const m=openModal(`
          <h3>Deploy Certificate</h3>
          <p class="text-sm text-muted mb-2">Deploy <strong>${escapeHtml(domain)}</strong> to an nginx site config.</p>
          <div class="field"><label>Site</label>
            ${sites.length
              ? `<select id="ssl-dep-site">${optsHtml}<option value="__custom__">Custom…</option></select>`
              : `<input type="text" id="ssl-dep-site" placeholder="example.com" value="${escapeHtml(domain)}">`}
          </div>
          <div class="field" id="ssl-dep-custom-wrap" style="display:none">
            <label>Custom site filename</label>
            <input type="text" id="ssl-dep-custom" placeholder="example.com" value="${escapeHtml(domain)}">
          </div>
          <div class="error-msg" id="ssl-dep-err" style="display:none"></div>
          <div class="btn-row">
            <button class="btn btn-sm btn-ghost" id="ssl-dep-cancel">Cancel</button>
            <button class="btn btn-sm btn-success" id="ssl-dep-ok">Deploy</button>
          </div>`,{maxWidth:'420px'});
        const siteEl=m.root.querySelector('#ssl-dep-site');
        const customWrap=m.root.querySelector('#ssl-dep-custom-wrap');
        if(siteEl&&siteEl.tagName==='SELECT'){
          siteEl.addEventListener('change',()=>{
            customWrap.style.display=siteEl.value==='__custom__'?'':'none';
          });
        }
        m.root.querySelector('#ssl-dep-cancel').onclick=m.close;
        m.root.querySelector('#ssl-dep-ok').onclick=async function(){
          let siteName='';
          if(siteEl.tagName==='SELECT'){
            siteName=siteEl.value==='__custom__'
              ?(m.root.querySelector('#ssl-dep-custom').value.trim())
              :siteEl.value;
          } else siteName=siteEl.value.trim();
          const errEl=m.root.querySelector('#ssl-dep-err');
          if(!siteName){errEl.textContent='Site name required';errEl.style.display='block';return}
          errEl.style.display='none';this.disabled=true;this.textContent='Deploying…';
          try{
            const r=await api('/ssl/deploy',{method:'POST',body:JSON.stringify({site_name:siteName,domain})});
            if(r.success){showToast('Deployed to '+siteName,'success');m.close();renderWebServerSsl(tc)}
            else{errEl.textContent=r.message||'Deploy failed';errEl.style.display='block';this.disabled=false;this.textContent='Deploy'}
          }catch(e){
            if(e.message!=='Unauthorized'){errEl.textContent=e.message;errEl.style.display='block'}
            this.disabled=false;this.textContent='Deploy';
          }
        };
      });
    });

    // Delete
    tc.querySelectorAll('.ssl-delete-btn').forEach(btn=>{
      btn.addEventListener('click',function(){
        const domain=this.dataset.domain;
        confirmDialog({
          title:'Delete Certificate',
          message:'Delete certificate for '+domain+'? This cannot be undone.',
          danger:true,
          okText:'Delete',
          onConfirm:async()=>{
            const r=await api('/ssl/certificates/'+encodeURIComponent(domain),{method:'DELETE'});
            if(!r.success)throw new Error(r.message||'Delete failed');
            showToast('Deleted','success');
            renderWebServerSsl(tc);
          }
        });
      });
    });
  }catch(e){
    if(e.message!=='Unauthorized')tc.innerHTML='<div class="error-msg">Error: '+e.message+'</div>';
  }
}

async function renderWebServerSslAccount(tc){
  tc.innerHTML=`
  <div class="card" style="max-width:500px">
    <h3 style="font-size:0.95rem;margin-bottom:1rem">Register ACME Account</h3>
    <div class="field">
      <label>Email</label>
      <input type="email" id="acct-email" placeholder="admin@example.com">
    </div>
    <div class="field">
      <label>ACME Provider</label>
      <select id="acct-provider">
        <option value="letsencrypt">Let's Encrypt</option>
        <option value="letsencrypt-staging">Let's Encrypt (Staging)</option>
        <option value="zerossl">ZeroSSL</option>
        <option value="buypass">Buypass</option>
        <option value="google">Google Public CA</option>
      </select>
    </div>
    <div class="field">
      <p class="text-sm text-muted">By registering, you agree to the ACME Subscriber Agreement (https://letsencrypt.org/repository/)</p>
    </div>
    <div id="acct-error" class="error-msg" style="display:none"></div>
    <div class="btn-row mt-3">
      <button class="btn btn-sm" id="acct-back">Back</button>
      <button class="btn btn-sm btn-success" id="acct-register">Register</button>
    </div>
  </div>`;

  document.getElementById('acct-back').addEventListener('click',()=>renderWebServerSsl(tc));
  document.getElementById('acct-register').addEventListener('click',async function(){
    const email=document.getElementById('acct-email').value.trim();
    const provider=document.getElementById('acct-provider').value;
    const errEl=document.getElementById('acct-error');
    if(!email){errEl.textContent='Email is required';errEl.style.display='block';return}
    errEl.style.display='none';
    this.disabled=true;this.textContent='Registering...';
    try{
      const r=await api('/ssl/accounts',{method:'POST',body:JSON.stringify({email,acme_provider:provider})});
      if(r.success){showToast('Account registered','success');renderWebServerSsl(tc)}
      else {errEl.textContent=r.message||'Registration failed';errEl.style.display='block';this.disabled=false;this.textContent='Register'}
    }catch(e){if(e.message!=='Unauthorized'){errEl.textContent='Error: '+e.message;errEl.style.display='block'}this.disabled=false;this.textContent='Register'}
  });
}

async function renderWebServerSslIssue(tc,providers){
  const provOpts=(providers||[]).map(p=>`<option value="${p.name}">${p.label}${p.is_staging?' (Staging)':''}</option>`).join('');
  tc.innerHTML=`
  <div class="card">
    <h3 style="font-size:0.95rem;margin-bottom:1rem">Issue SSL Certificate</h3>
    <div class="form-row">
      <div class="field">
        <label>ACME Provider</label>
        <select id="ssl-acme-provider">${provOpts||'<option value="letsencrypt">Let\'s Encrypt</option>'}</select>
      </div>
      <div class="field" style="flex:2">
        <label>Domain(s)</label>
        <input type="text" id="ssl-domain" placeholder="example.com" style="width:100%">
        <div class="text-sm text-dim mt-1">Separate multiple with commas</div>
      </div>
    </div>
    <div class="form-row">
      <div class="field">
        <label>Challenge Type</label>
        <select id="ssl-challenge">
          <option value="http">HTTP-01 (port 80)</option>
          <option value="dns">DNS-01 (wildcard)</option>
        </select>
      </div>
      <div class="field">
        <label>Email</label>
        <input type="email" id="ssl-email" placeholder="admin@example.com">
      </div>
    </div>
    <div id="ssl-http-options">
      <div class="field">
        <label>Webroot (optional, for HTTP validation)</label>
        <input type="text" id="ssl-webroot" placeholder="/var/www/html">
        <div class="text-sm text-dim mt-1">Leave empty for standalone mode (certbot will bind port 80)</div>
      </div>
    </div>
    <div id="ssl-dns-options" style="display:none">
      <div class="field">
        <label>DNS Plugin (optional)</label>
        <select id="ssl-dns-plugin">
          <option value="">Manual DNS (add TXT record yourself)</option>
          <option value="cloudflare">Cloudflare</option>
          <option value="aliyun">Aliyun DNS</option>
          <option value="dnspod">DNSPod</option>
          <option value="godaddy">GoDaddy</option>
          <option value="aws">AWS Route53</option>
          <option value="digitalocean">DigitalOcean</option>
        </select>
      </div>
      <div class="field">
        <label>DNS Credentials File Path (for selected plugin)</label>
        <input type="text" id="ssl-dns-creds" placeholder="/etc/letsencrypt/cloudflare.ini">
      </div>
    </div>
    <div id="ssl-issue-error" class="error-msg" style="display:none"></div>
    <div id="ssl-issue-result" style="display:none"></div>
    <div class="btn-row mt-3">
      <button class="btn btn-sm" id="ssl-issue-back">Back</button>
      <button class="btn btn-sm btn-success" id="ssl-issue-start">Issue Certificate</button>
    </div>
  </div>`;

  // Toggle DNS options
  document.getElementById('ssl-challenge').addEventListener('change',function(){
    const isDns=this.value==='dns';
    document.getElementById('ssl-http-options').style.display=isDns?'none':'';
    document.getElementById('ssl-dns-options').style.display=isDns?'':'none';
  });

  document.getElementById('ssl-issue-back').addEventListener('click',()=>renderWebServerSsl(tc));
  document.getElementById('ssl-issue-start').addEventListener('click',async function(){
    const domainStr=document.getElementById('ssl-domain').value.trim();
    const email=document.getElementById('ssl-email').value.trim();
    const webroot=document.getElementById('ssl-webroot').value.trim();
    const provider=document.getElementById('ssl-acme-provider').value;
    const challenge=document.getElementById('ssl-challenge').value;
    const dnsPlugin=document.getElementById('ssl-dns-plugin').value;
    const dnsCreds=document.getElementById('ssl-dns-creds').value.trim();
    const errEl=document.getElementById('ssl-issue-error');
    const resEl=document.getElementById('ssl-issue-result');

    if(!domainStr){errEl.textContent='At least one domain is required';errEl.style.display='block';return}
    errEl.style.display='none';resEl.style.display='none';

    const domains=domainStr.split(',').map(s=>s.trim()).filter(Boolean);
    const btn=this;btn.disabled=true;btn.textContent='Issuing...';
    try{
      const body={domains,acme_provider:provider,challenge};
      if(email)body.email=email;
      if(challenge==='dns'){
        if(dnsPlugin)body.dns_plugin=dnsPlugin;
        if(dnsCreds)body.dns_credentials=dnsCreds;
      } else {
        if(webroot)body.webroot=webroot;
      }
      const r=await api('/ssl/issue',{method:'POST',body:JSON.stringify(body)});
      if(r.success){
        showToast('Certificate issued','success');
        renderWebServerSsl(tc);
      } else {
        errEl.textContent=r.message||'Issue failed';
        errEl.style.display='block';btn.disabled=false;btn.textContent='Issue Certificate';
      }
    }catch(e){
      if(e.message!=='Unauthorized'){
        errEl.textContent='Error: '+e.message;errEl.style.display='block';btn.disabled=false;btn.textContent='Issue Certificate';
      }
    }
  });
}

// ── SSL Upload Certificate ──
function renderWebServerSslUpload(tc){
  tc.innerHTML=`
  <div class="card" style="max-width:600px">
    <h3 style="font-size:0.95rem;margin-bottom:1rem">Upload Certificate</h3>
    <div class="field">
      <label>Name</label>
      <input type="text" id="ssl-up-name" placeholder="my-certificate">
    </div>
    <div class="field">
      <label>Certificate (PEM)</label>
      <textarea id="ssl-up-cert" rows="6" class="config-area" placeholder="-----BEGIN CERTIFICATE-----\n..."></textarea>
    </div>
    <div class="field">
      <label>Private Key (PEM)</label>
      <textarea id="ssl-up-key" rows="6" class="config-area" placeholder="-----BEGIN PRIVATE KEY-----\n..."></textarea>
    </div>
    <div id="ssl-up-error" class="error-msg" style="display:none"></div>
    <div class="btn-row mt-3">
      <button class="btn btn-sm" id="ssl-up-back">Back</button>
      <button class="btn btn-sm btn-success" id="ssl-up-do">Upload</button>
    </div>
  </div>`;
  document.getElementById('ssl-up-back').addEventListener('click',()=>renderWebServerSsl(tc));
  document.getElementById('ssl-up-do').addEventListener('click',async function(){
    const name=document.getElementById('ssl-up-name').value.trim();
    const cert=document.getElementById('ssl-up-cert').value.trim();
    const key=document.getElementById('ssl-up-key').value.trim();
    const errEl=document.getElementById('ssl-up-error');
    if(!name||!cert||!key){errEl.textContent='All fields are required';errEl.style.display='block';return}
    errEl.style.display='none';this.disabled=true;this.textContent='Uploading...';
    try{
      const r=await api('/ssl/upload',{method:'POST',body:JSON.stringify({name,cert,key})});
      if(r.success){showToast('Certificate uploaded','success');renderWebServerSsl(tc)}
      else{errEl.textContent=r.message||'Upload failed';errEl.style.display='block';this.disabled=false;this.textContent='Upload'}
    }catch(e){if(e.message!=='Unauthorized'){errEl.textContent='Error: '+e.message;errEl.style.display='block'}this.disabled=false;this.textContent='Upload'}
  });
}

// ── SSL Self-Signed Certificate ──
function renderWebServerSslSelfSigned(tc){
  tc.innerHTML=`
  <div class="card" style="max-width:500px">
    <h3 style="font-size:0.95rem;margin-bottom:1rem">Self-Signed Certificate</h3>
    <div class="field">
      <label>Domain / CN</label>
      <input type="text" id="ssl-ss-domain" placeholder="example.com">
    </div>
    <div class="field">
      <label>Validity (days)</label>
      <input type="number" id="ssl-ss-days" value="365" min="1" max="3650">
    </div>
    <div id="ssl-ss-error" class="error-msg" style="display:none"></div>
    <div class="btn-row mt-3">
      <button class="btn btn-sm" id="ssl-ss-back">Back</button>
      <button class="btn btn-sm btn-success" id="ssl-ss-do">Generate</button>
    </div>
  </div>`;
  document.getElementById('ssl-ss-back').addEventListener('click',()=>renderWebServerSsl(tc));
  document.getElementById('ssl-ss-do').addEventListener('click',async function(){
    const domain=document.getElementById('ssl-ss-domain').value.trim();
    const days=parseInt(document.getElementById('ssl-ss-days').value)||365;
    const errEl=document.getElementById('ssl-ss-error');
    if(!domain){errEl.textContent='Domain is required';errEl.style.display='block';return}
    errEl.style.display='none';this.disabled=true;this.textContent='Generating...';
    try{
      const r=await api('/ssl/self-signed',{method:'POST',body:JSON.stringify({domain,days})});
      if(r.success){showToast('Self-signed certificate generated','success');renderWebServerSsl(tc)}
      else{errEl.textContent=r.message||'Generation failed';errEl.style.display='block';this.disabled=false;this.textContent='Generate'}
    }catch(e){if(e.message!=='Unauthorized'){errEl.textContent='Error: '+e.message;errEl.style.display='block'}this.disabled=false;this.textContent='Generate'}
  });
}


// ── Firewall ──
async function renderFirewall(){
  const c=document.getElementById('content');
  if(!c)return;
  c.innerHTML='<div style="text-align:center;padding:2rem;color:var(--text-dim)">Loading firewall...</div>';
  try{
    const [statusJ, rulesJ] = await Promise.all([
      api('/firewall/status'),
      api('/firewall/rules')
    ]);
    const statusData = statusJ.data||{};
    const rules = rulesJ.data||[];
    if(!Array.isArray(rules)){c.innerHTML='<div class="error-msg">Invalid firewall data</div>';return}

    const iptablesOk=!!statusData.iptables_available;
    const ufwOk=!!statusData.ufw_available;
    const ufwActive=statusData.ufw_active===true;
    const active=ufwActive||iptablesOk;

    let html=`
    <div class="flex justify-between items-center mb-4">
      <h2 style="font-size:1.1rem;font-weight:600">Firewall</h2>
      <div class="flex items-center gap-3">
        <span class="text-sm text-muted">iptables: ${iptablesOk?'available':'missing'}${ufwOk?' · ufw '+(ufwActive?'active':'inactive'):''}</span>
        <span class="status-badge ${active?'running':'exited'}">${active?'Ready':'Unavailable'}</span>
      </div>
    </div>`;

    html+=`<div class="card mb-4">
      <h3 style="font-size:0.95rem;margin-bottom:.75rem;color:var(--text-muted)">Rules</h3>
      <div class="table-wrap"><table>
        <thead><tr><th>Chain</th><th>#</th><th>Target</th><th>Prot</th><th>Source</th><th>Destination</th><th></th></tr></thead>
        <tbody>`;
    if(rules.length===0){
      html+=`<tr><td colspan="7" class="text-center text-muted">No rules</td></tr>`;
    } else {
      rules.forEach(rule=>{
        const chain=rule.chain||rule.chaine||'';
        const num=rule.num||rule.number||rule.rule_num||'';
        const target=rule.target||'';
        const prot=rule.prot||rule.protocol||'';
        const source=rule.source||rule.src||'';
        const dest=rule.destination||rule.dst||'';
        html+=`<tr>
          <td>${escapeHtml(chain)}</td>
          <td>${num}</td>
          <td>${escapeHtml(target)}</td>
          <td>${escapeHtml(prot)}</td>
          <td style="font-family:monospace;font-size:0.8rem">${escapeHtml(source)}</td>
          <td style="font-family:monospace;font-size:0.8rem">${escapeHtml(dest)}</td>
          <td><button class="btn btn-xs btn-danger fw-del-row" data-chain="${escapeHtml(chain)}" data-num="${num}">Delete</button></td>
        </tr>`;
      });
    }
    html+=`</tbody></table></div></div>`;

    html+=`
    <div class="card-grid card-grid-3">
      <div class="card">
        <h3 style="font-size:0.95rem;margin-bottom:.75rem;color:var(--text-muted)">Add Rule</h3>
        <div class="field">
          <label>Chain</label>
          <select id="fw-add-chain">
            <option value="INPUT">INPUT</option>
            <option value="OUTPUT">OUTPUT</option>
            <option value="FORWARD">FORWARD</option>
          </select>
        </div>
        <div class="field">
          <label>Rule</label>
          <input type="text" id="fw-add-rule" placeholder="-s 192.168.1.0/24 -j ACCEPT">
        </div>
        <button class="btn btn-sm btn-success" id="fw-add-rule-btn">Add Rule</button>
        <div id="fw-add-error" class="error-msg" style="display:none"></div>
      </div>
      <div class="card">
        <h3 style="font-size:0.95rem;margin-bottom:.75rem;color:var(--text-muted)">Delete Rule</h3>
        <div class="field">
          <label>Chain</label>
          <select id="fw-del-chain">
            <option value="INPUT">INPUT</option>
            <option value="OUTPUT">OUTPUT</option>
            <option value="FORWARD">FORWARD</option>
          </select>
        </div>
        <div class="field">
          <label>Rule Number</label>
          <input type="number" id="fw-del-num" min="1" placeholder="1">
        </div>
        <button class="btn btn-sm btn-danger" id="fw-del-rule-btn">Delete Rule</button>
        <div id="fw-del-error" class="error-msg" style="display:none"></div>
      </div>
      <div class="card">
        <h3 style="font-size:0.95rem;margin-bottom:.75rem;color:var(--text-muted)">Open Port</h3>
        <div class="field">
          <label>Port</label>
          <input type="number" id="fw-port-num" min="1" max="65535" placeholder="80">
        </div>
        <div class="field">
          <label>Protocol</label>
          <select id="fw-port-prot">
            <option value="tcp">TCP</option>
            <option value="udp">UDP</option>
            <option value="both">Both</option>
          </select>
        </div>
        <button class="btn btn-sm btn-success" id="fw-open-port-btn">Open Port</button>
        <div id="fw-port-error" class="error-msg" style="display:none"></div>
      </div>
    </div>`;

    c.innerHTML=html;

    document.getElementById('fw-add-rule-btn').addEventListener('click',async()=>{
      const chain=document.getElementById('fw-add-chain').value;
      const rule=document.getElementById('fw-add-rule').value.trim();
      const errEl=document.getElementById('fw-add-error');
      if(!rule){errEl.textContent='Rule is required';errEl.style.display='block';return}
      errEl.style.display='none';
      const btn=document.getElementById('fw-add-rule-btn');
      btn.disabled=true;btn.textContent='Adding...';
      try{
        const r=await api('/firewall/rules',{
          method:'POST',
          body:JSON.stringify({chain,rule})
        });
        if(r.success){
          showToast('Rule added','success');
          renderFirewall();
        } else {
          errEl.textContent=r.message||'Failed to add rule';
          errEl.style.display='block';
          btn.disabled=false;btn.textContent='Add Rule';
        }
      }catch(e){
        if(e.message!=='Unauthorized'){
          errEl.textContent='Error: '+e.message;
          errEl.style.display='block';
          btn.disabled=false;btn.textContent='Add Rule';
        }
      }
    });

    const deleteRule=async(chain,num,errEl,btn)=>{
      if(errEl){errEl.style.display='none'}
      if(btn){btn.disabled=true;const orig=btn.textContent;btn.textContent='…';btn.dataset.orig=orig}
      try{
        const r=await api('/firewall/rules/'+encodeURIComponent(chain)+'?num='+encodeURIComponent(num),{method:'DELETE'});
        if(r.success){showToast('Rule deleted','success');renderFirewall();return}
        if(errEl){errEl.textContent=r.message||'Failed to delete rule';errEl.style.display='block'}
        else showToast(r.message||'Delete failed','error');
      }catch(e){
        if(e.message!=='Unauthorized'){
          if(errEl){errEl.textContent='Error: '+e.message;errEl.style.display='block'}
          else showToast('Error: '+e.message,'error');
        }
      }finally{
        if(btn){btn.disabled=false;btn.textContent=btn.dataset.orig||'Delete Rule'}
      }
    };

    document.getElementById('fw-del-rule-btn').addEventListener('click',async()=>{
      const chain=document.getElementById('fw-del-chain').value;
      const num=document.getElementById('fw-del-num').value;
      const errEl=document.getElementById('fw-del-error');
      if(!num){errEl.textContent='Rule number is required';errEl.style.display='block';return}
      await deleteRule(chain,num,errEl,document.getElementById('fw-del-rule-btn'));
    });
    c.querySelectorAll('.fw-del-row').forEach(btn=>{
      btn.onclick=()=>deleteRule(btn.dataset.chain,btn.dataset.num,null,btn);
    });

    document.getElementById('fw-open-port-btn').addEventListener('click',async()=>{
      const port=document.getElementById('fw-port-num').value;
      const prot=document.getElementById('fw-port-prot').value;
      const errEl=document.getElementById('fw-port-error');
      if(!port||port<1||port>65535){errEl.textContent='Valid port (1-65535) is required';errEl.style.display='block';return}
      errEl.style.display='none';
      const btn=document.getElementById('fw-open-port-btn');
      btn.disabled=true;btn.textContent='Opening...';
      try{
        const protos=prot==='both'?['tcp','udp']:[prot];
        let last=null;
        for(const p of protos){
          last=await api('/firewall/port',{
            method:'POST',
            body:JSON.stringify({port:parseInt(port),protocol:p})
          });
          if(!last.success)break;
        }
        if(last&&last.success){
          showToast('Port opened','success');
          renderFirewall();
        } else {
          errEl.textContent=(last&&last.message)||'Failed to open port';
          errEl.style.display='block';
          btn.disabled=false;btn.textContent='Open Port';
        }
      }catch(e){
        if(e.message!=='Unauthorized'){
          errEl.textContent='Error: '+e.message;
          errEl.style.display='block';
          btn.disabled=false;btn.textContent='Open Port';
        }
      }
    });

  }catch(e){
    if(e.message!=='Unauthorized')c.innerHTML=`<div class="error-msg">Error: ${e.message}</div>`;
  }
}

// ── Software Installer ──
const APP_META={
  nginx:{icon:'🌐',cat:'Web Server'},
  mysql:{icon:'🗄️',cat:'Database'},
  pgsql:{icon:'🐘',cat:'Database'},
  redis:{icon:'🧩',cat:'Cache'},
  docker:{icon:'🐳',cat:'Runtime'}
};
let appFilterQ='';
let appFilterCat='all';
async function renderInstaller(){
  const c=document.getElementById('content');
  if(!c)return;
  c.innerHTML=`
  <div class="section-header">
    <div>
      <h2 style="font-size:1.15rem;font-weight:700">App Store</h2>
      <div class="text-sm text-dim" style="margin-top:2px">One-click install of common server software</div>
    </div>
  </div>
  <div class="dk-toolbar" id="app-toolbar" style="display:none">
    <div class="dk-toolbar-left">
      <input type="search" class="dk-search" id="app-search" placeholder="Search apps…" value="${escapeHtml(appFilterQ)}" autocomplete="off">
      <div class="dk-chips" id="app-chips"></div>
    </div>
    <span class="text-sm text-muted" id="app-count"></span>
  </div>
  <div id="installer-list">
    <div class="app-grid">
      <div class="app-card"><div class="skeleton skeleton-text" style="width:55%"></div><div class="skeleton skeleton-text"></div></div>
      <div class="app-card"><div class="skeleton skeleton-text" style="width:55%"></div><div class="skeleton skeleton-text"></div></div>
      <div class="app-card"><div class="skeleton skeleton-text" style="width:55%"></div><div class="skeleton skeleton-text"></div></div>
    </div>
  </div>`;
  try{
    const j=await api('/installer/list');
    if(!j.success||!Array.isArray(j.data)){document.getElementById('installer-list').innerHTML='<div class="error-msg">Failed to load software list</div>';return}
    const items=j.data.map(s=>{
      const m=APP_META[s.name]||{icon:'📦',cat:'Software'};
      return {...s,icon:m.icon,cat:m.cat};
    });
    const cats=['all',...Array.from(new Set(items.map(i=>i.cat)))];
    const toolbar=document.getElementById('app-toolbar');
    toolbar.style.display='flex';
    document.getElementById('app-chips').innerHTML=cats.map(cat=>{
      const n=cat==='all'?items.length:items.filter(i=>i.cat===cat).length;
      const label=cat==='all'?'All':cat;
      return `<button type="button" class="dk-chip${appFilterCat===cat?' active':''}" data-cat="${escapeHtml(cat)}">${escapeHtml(label)} <span>${n}</span></button>`;
    }).join('');
    const paint=()=>{
      const q=appFilterQ.trim().toLowerCase();
      const filtered=items.filter(s=>{
        if(appFilterCat!=='all'&&s.cat!==appFilterCat)return false;
        if(!q)return true;
        return (s.name||'').toLowerCase().includes(q)||(s.description||'').toLowerCase().includes(q)||(s.cat||'').toLowerCase().includes(q);
      });
      document.getElementById('app-count').textContent=filtered.length+' app(s)';
      document.querySelectorAll('#app-chips .dk-chip').forEach(b=>b.classList.toggle('active',b.dataset.cat===appFilterCat));
      if(filtered.length===0){
        document.getElementById('installer-list').innerHTML='<div class="empty-state"><div class="icon">🔎</div><div>No apps match</div></div>';
        return;
      }
      let html='<div class="app-grid">';
      filtered.forEach(s=>{
        html+=`<div class="app-card">
          <div class="app-top">
            <div class="app-icon">${s.icon}</div>
            <div class="app-head">
              <div class="app-name">${escapeHtml(s.name)}</div>
              <span class="app-cat">${escapeHtml(s.cat)}</span>
            </div>
          </div>
          <div class="app-desc">${escapeHtml(s.description)}</div>
          <div class="app-actions">
            <button class="btn btn-sm btn-success inst-install" data-soft="${escapeHtml(s.name)}">Install</button>
            <button class="btn btn-sm btn-ghost inst-uninstall" data-soft="${escapeHtml(s.name)}">Uninstall</button>
          </div>
        </div>`;
      });
      html+='</div>';
      document.getElementById('installer-list').innerHTML=html;
      document.querySelectorAll('.inst-install').forEach(btn=>{
        btn.addEventListener('click',function(){installSoftware(this.dataset.soft)});
      });
      document.querySelectorAll('.inst-uninstall').forEach(btn=>{
        btn.addEventListener('click',function(){uninstallSoftware(this.dataset.soft)});
      });
    };
    document.getElementById('app-search').oninput=function(){appFilterQ=this.value;paint()};
    document.getElementById('app-chips').onclick=e=>{
      const btn=e.target.closest('.dk-chip');if(!btn)return;
      appFilterCat=btn.dataset.cat||'all';paint();
    };
    paint();
  }catch(e){
    if(e.message!=='Unauthorized')document.getElementById('installer-list').innerHTML='<div class="error-msg">Error: '+e.message+'</div>';
  }
}

function installSoftware(soft){
  const m=openModal(`
    <h3>Install ${escapeHtml(soft)}</h3>
    <div class="field">
      <label>Version</label>
      <select id="inst-version"><option value="">Loading versions…</option></select>
    </div>
    <div class="text-xs text-dim" id="inst-hint" style="margin:-4px 0 6px">Fetching versions from the official upstream…</div>
    <div class="error-msg" id="inst-err" style="display:none"></div>
    <div class="btn-row">
      <button class="btn btn-sm btn-ghost" id="inst-cancel">Cancel</button>
      <button class="btn btn-sm btn-success" id="inst-go" disabled>Install</button>
    </div>`,{maxWidth:'420px'});
  const go=m.root.querySelector('#inst-go');
  const err=m.root.querySelector('#inst-err');
  const hint=m.root.querySelector('#inst-hint');
  m.root.querySelector('#inst-cancel').addEventListener('click',m.close);

  function useManualInput(msg){
    const field=m.root.querySelector('#inst-version').parentElement;
    field.innerHTML='<label>Version</label><input type="text" id="inst-version" placeholder="latest">';
    hint.textContent=msg;
  }

  // Populate the version dropdown from the software's upstream source.
  (async()=>{
    try{
      const r=await api('/installer/versions?software='+encodeURIComponent(soft));
      const list=(r&&r.success&&Array.isArray(r.data))?r.data:[];
      if(list.length>0){
        const sel=m.root.querySelector('#inst-version');
        sel.innerHTML='<option value="">latest</option>'+list.map(v=>`<option value="${escapeHtml(v)}">${escapeHtml(v)}</option>`).join('');
        hint.textContent=list.length+' versions from the official upstream source (blank = latest).';
      }else{
        useManualInput('No upstream version list — enter a version manually (blank = latest).');
      }
    }catch(e){
      if(e.message==='Unauthorized'){m.close();return}
      useManualInput('Could not fetch versions — enter a version manually (blank = latest).');
    }finally{
      go.disabled=false;
    }
  })();

  go.addEventListener('click',async()=>{
    const el=m.root.querySelector('#inst-version');
    const version=((el&&el.value)||'').trim();
    go.disabled=true;go.textContent='Installing...';err.style.display='none';
    try{
      const r=await api('/installer/install',{method:'POST',body:JSON.stringify({software:soft,version:version||undefined})});
      if(r.success){showToast(soft+' installed successfully','success');m.close()}
      else{err.textContent=r.message||'Install failed';err.style.display='block';go.disabled=false;go.textContent='Install'}
    }catch(e){
      if(e.message==='Unauthorized'){m.close();return}
      err.textContent='Error: '+e.message;err.style.display='block';go.disabled=false;go.textContent='Install';
    }
  });
}

function uninstallSoftware(soft){
  confirmDialog({
    title:'Uninstall '+soft,
    message:'Are you sure you want to uninstall '+soft+'?',
    okText:'Uninstall',danger:true,loadingText:'Uninstalling...',
    onConfirm:async()=>{
      const r=await api('/installer/uninstall',{method:'POST',body:JSON.stringify({software:soft})});
      if(!r.success)throw new Error(r.message||'Uninstall failed');
      showToast(soft+' uninstalled successfully','success');
    }
  });
}
