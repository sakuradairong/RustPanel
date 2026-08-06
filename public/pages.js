
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
    <button class="btn btn-sm" id="file-save-btn" disabled>Save</button>
    <button class="btn btn-sm" id="file-back-btn">Back</button>
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
  <div class="flex justify-between items-center mb-4">
    <h2 style="font-size:1.1rem;font-weight:600">Docker</h2>
    <div>
      <button class="btn btn-sm ${dockerTab==='containers'?'btn-success':''}" id="dt-con">Containers</button>
      <button class="btn btn-sm" id="dt-img">Images</button>
      <button class="btn btn-sm" id="dt-vol" style="margin-left:2px">Volumes</button>
      <button class="btn btn-sm" id="dt-net">Networks</button>
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
async function renderDockerContainers(dc){
  dc.innerHTML='<div style="text-align:center;padding:2rem;color:var(--text-dim)">Loading...</div>';
  try{
    const j=await api('/docker/containers?all=true');
    if(!j.success){dc.innerHTML='<div class="error-msg">Failed</div>';return}
    const list=j.data||[];
    if(!Array.isArray(list)){dc.innerHTML='<div class="error-msg">Invalid data</div>';return}
    let html='<div class="flex justify-between items-center mb-2"><span style="font-size:0.85rem;color:var(--text-dim)">'+list.length+' container(s)</span><div><button class="btn btn-sm btn-success" id="dc-create-btn">+ Create</button><button class="btn btn-sm" id="dc-refresh" style="margin-left:4px">⟳</button></div></div>';
    html+='<div class="card"><div class="table-wrap"><table><thead><tr><th>ID</th><th>Name</th><th>Image</th><th>State</th><th>Ports</th><th>Actions</th></tr></thead><tbody>';
    list.forEach(ct=>{
      const shortId=(ct.id||'').substring(0,12);
      const st=ct.state||'';
      const sc=st.toLowerCase();
      const ports=Array.isArray(ct.ports)?ct.ports.join(', '):(ct.ports||'');
      const nm=(ct.name||'').replace(/^\//,'');
      const run=st==='running';const paused=st==='paused';
      html+=`<tr>
        <td style="font-family:monospace;font-size:0.8rem">${shortId}</td>
        <td><strong>${escapeHtml(nm)}</strong><br><span style="font-size:0.75rem;color:var(--text-dim)">${escapeHtml(ct.status||'')}</span></td>
        <td style="color:var(--text-muted);max-width:180px;overflow:hidden;text-overflow:ellipsis">${escapeHtml(ct.image||'')}</td>
        <td><span class="status-badge ${sc}">${st}</span></td>
        <td style="font-size:0.75rem;color:var(--text-dim);max-width:150px;overflow:hidden;text-overflow:ellipsis">${escapeHtml(ports)}</td>
        <td style="white-space:nowrap">
          <button class="btn btn-sm btn-success dc-act" data-id="${ct.id}" data-act="start" ${run?'disabled':''} title="Start">▶</button>
          <button class="btn btn-sm btn-warning dc-act" data-id="${ct.id}" data-act="stop" ${!run||paused?'disabled':''} title="Stop">⏹</button>
          <button class="btn btn-sm dc-act" data-id="${ct.id}" data-act="restart" title="Restart">↻</button>
          <button class="btn btn-sm dc-act" data-id="${ct.id}" data-act="logs" title="Logs">📋</button>
          <br><span style="font-size:0">
          <button class="btn btn-sm dc-act" data-id="${ct.id}" data-act="stats" title="Stats">📊</button>
          <button class="btn btn-sm dc-act" data-id="${ct.id}" data-act="inspect" title="Inspect">🔍</button>
        </span></td>
      </tr>`;
    });
    html+='</tbody></table></div></div>';
    dc.innerHTML=html;
    document.getElementById('dc-refresh').onclick=()=>renderDockerContainers(dc);
    document.getElementById('dc-create-btn').onclick=()=>renderDockerCreate(dc);
    dc.querySelectorAll('.dc-act').forEach(btn=>{
      btn.onclick=async function(){
        const id=this.dataset.id;const act=this.dataset.act;
        if(!id)return;
        if(act==='logs'){renderDockerLogs(dc,id);return}
        if(act==='stats'){renderDockerStats(dc,id);return}
        if(act==='inspect'){renderDockerInspect(dc,id);return}
        this.disabled=true;const orig=this.textContent;this.textContent='...';
        try{
          const r=await api('/docker/containers/'+encodeURIComponent(id)+'/'+act,{method:'POST'});
          if(!r.success)showToast((r.message||'Failed'),'error');
          renderDockerContainers(dc);
        }catch(e){if(e.message!=='Unauthorized')showToast('Error: '+e.message,'error');this.disabled=false;this.textContent=orig}
      };
    });
  }catch(e){if(e.message!=='Unauthorized')dc.innerHTML='<div class="error-msg">Error: '+e.message+'</div>'}
}

async function renderDockerLogs(dc,id){
  await loadDockerLogs(dc,id,200);
}
async function loadDockerLogs(dc,id,tail){
  dc.innerHTML='<div style="text-align:center;padding:2rem;color:var(--text-dim)">Loading logs...</div>';
  try{
    const j=await api('/docker/containers/'+encodeURIComponent(id)+'/logs?tail='+tail);
    dc.innerHTML=`
    <div class="flex justify-between items-center mb-2">
      <span style="font-size:0.85rem;font-weight:600">Logs</span>
      <div class="flex items-center gap-2">
        <span class="text-sm text-muted">Lines:</span>
        <select id="dc-log-tail" style="width:auto;padding:2px 8px" onchange="loadDockerLogs(document.getElementById('docker-content'),'${id}',this.value)">
          <option value="50" ${tail==50?'selected':''}>50</option>
          <option value="200" ${tail==200?'selected':''}>200</option>
          <option value="1000" ${tail==1000?'selected':''}>1000</option>
          <option value="5000" ${tail==5000?'selected':''}>5000</option>
        </select>
        <button class="btn btn-sm" id="dc-logs-back">Back</button>
      </div>
    </div>
    <pre style="background:#1a1a2e;color:#e0e0e0;padding:12px;border-radius:6px;font-size:0.8rem;max-height:600px;overflow-y:auto;font-family:monospace;white-space:pre-wrap">${escapeHtml(j.data?.logs||'')||'<span style="color:var(--text-dim)">No logs</span>'}</pre>`;
    document.getElementById('dc-logs-back').onclick=()=>renderDockerContainers(dc);
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

// ── Container Inspect ──
async function renderDockerInspect(dc,id){
  dc.innerHTML='<div style="text-align:center;padding:2rem;color:var(--text-dim)">Loading inspect...</div>';
  try{
    const j=await api('/docker/containers/'+encodeURIComponent(id)+'/inspect');
    if(!j.success||!j.data){dc.innerHTML='<div class="error-msg">Inspect data unavailable</div>';return}
    dc.innerHTML=`
    <div class="flex justify-between items-center mb-3">
      <span style="font-size:0.85rem;font-weight:600">Container Inspect</span>
      <button class="btn btn-sm" id="dc-ins-back">Back</button>
    </div>
    <pre style="background:#1a1a2e;color:#e0e0e0;padding:12px;border-radius:6px;font-size:0.8rem;max-height:600px;overflow-y:auto;font-family:monospace;white-space:pre-wrap">${escapeHtml(JSON.stringify(j.data,null,2))}</pre>`;
    document.getElementById('dc-ins-back').onclick=()=>renderDockerContainers(dc);
  }catch(e){if(e.message!=='Unauthorized')dc.innerHTML='<div class="error-msg">Error: '+e.message+'</div>'}
}

async function renderDockerCreate(dc){
  dc.innerHTML=`
  <div class="card" style="max-width:500px">
    <h3 style="font-size:0.95rem;margin-bottom:1rem">Create Container</h3>
    <div class="field"><label>Image</label><input type="text" id="dc-img" placeholder="nginx:latest"></div>
    <div class="field"><label>Name</label><input type="text" id="dc-name" placeholder="my-nginx"></div>
    <div class="field"><label>Command</label><input type="text" id="dc-cmd" placeholder="optional"></div>
    <div id="dc-err" class="error-msg" style="display:none"></div>
    <div class="btn-row mt-3">
      <button class="btn btn-sm" id="dc-c-back">Back</button>
      <button class="btn btn-sm btn-success" id="dc-c-do">Create & Start</button>
    </div>
  </div>`;
  document.getElementById('dc-c-back').onclick=()=>renderDockerContainers(dc);
  document.getElementById('dc-c-do').onclick=async function(){
    const img=document.getElementById('dc-img').value.trim();
    const nm=document.getElementById('dc-name').value.trim();
    const cmdStr=document.getElementById('dc-cmd').value.trim();
    const err=document.getElementById('dc-err');
    if(!img||!nm){err.textContent='Image and name required';err.style.display='block';return}
    err.style.display='none';
    this.disabled=true;this.textContent='Creating...';
    try{
      const body={name:nm,image:img};
      if(cmdStr)body.cmd=cmdStr.split(/\s+/);
      const r=await api('/docker/containers',{method:'POST',body:JSON.stringify(body)});
      if(r.success){
        showToast('Created: '+nm,'success');
        if(r.data?.container_id) await api('/docker/containers/'+encodeURIComponent(r.data.container_id)+'/start',{method:'POST'});
        renderDockerContainers(dc);
      } else {err.textContent=r.message||'Failed';err.style.display='block';this.disabled=false;this.textContent='Create & Start'}
    }catch(e){if(e.message!=='Unauthorized'){err.textContent='Error: '+e.message;err.style.display='block'}this.disabled=false;this.textContent='Create & Start'}
  };
}

// ── Images ──
async function renderDockerImages(dc){
  dc.innerHTML='<div style="text-align:center;padding:2rem;color:var(--text-dim)">Loading images...</div>';
  try{
    const j=await api('/docker/images');
    if(!j.success){dc.innerHTML='<div class="error-msg">Failed</div>';return}
    const list=j.data||[];if(!Array.isArray(list)){dc.innerHTML='<div class="error-msg">Invalid</div>';return}
    let html='<div class="flex justify-between items-center mb-2"><span style="font-size:0.85rem;color:var(--text-dim)">'+list.length+' image(s)</span><button class="btn btn-sm btn-success" id="di-pull">+ Pull</button></div>';
    html+='<div class="card"><div class="table-wrap"><table><thead><tr><th>Repository</th><th>Tag</th><th>ID</th><th>Size</th><th>Actions</th></tr></thead><tbody>';
    list.forEach(img=>{
      const tags=img.repo_tags||[];
      const repo=tags.length>0?(tags[0].split(':')[0]||'<none>'):'<none>';
      const tag=tags.length>0?(tags[0].split(':')[1]||'<none>'):'<none>';
      const sid=(img.id||'').substring(0,12);
      const sz=img.size||0;
      const szStr=sz>1073741824?(sz/1073741824).toFixed(2)+' GB':sz>1048576?(sz/1048576).toFixed(1)+' MB':(sz/1024).toFixed(0)+' KB';
      html+=`<tr><td>${escapeHtml(repo)}</td><td style="color:var(--text-dim)">${escapeHtml(tag)}</td><td style="font-family:monospace;font-size:0.8rem;color:var(--text-muted)">${sid}</td><td style="font-size:0.8rem">${szStr}</td><td><button class="btn btn-sm btn-danger di-rm" data-id="${img.id||''}">✕</button></td></tr>`;
    });
    html+='</tbody></table></div></div>';
    dc.innerHTML=html;
    document.getElementById('di-pull').onclick=()=>renderDockerPull(dc);
    dc.querySelectorAll('.di-rm').forEach(btn=>{btn.onclick=async function(){
      const id=this.dataset.id;if(!confirm('Remove?'))return;
      this.disabled=true;this.textContent='...';
      try{const r=await api('/docker/images/'+encodeURIComponent(id)+'/remove',{method:'POST'});if(r.success){showToast('Removed','success');renderDockerImages(dc)}else showToast(r.message||'Failed','error')}catch(e){if(e.message!=='Unauthorized')showToast('Error: '+e.message,'error')}
    };});
  }catch(e){if(e.message!=='Unauthorized')dc.innerHTML='<div class="error-msg">Error: '+e.message+'</div>'}
}

async function renderDockerPull(dc){
  dc.innerHTML=`
  <div class="card" style="max-width:450px">
    <h3 style="font-size:0.95rem;margin-bottom:1rem">Pull Image</h3>
    <div class="field"><label>Image</label><input type="text" id="di-pull-img" placeholder="nginx:latest"></div>
    <div id="di-pull-out" style="display:none;background:var(--bg);padding:8px;border-radius:4px;font-size:0.8rem;max-height:150px;overflow-y:auto"></div>
    <div id="di-pull-err" class="error-msg" style="display:none"></div>
    <div class="btn-row mt-3">
      <button class="btn btn-sm" id="di-pull-back">Back</button>
      <button class="btn btn-sm btn-success" id="di-pull-go">Pull</button>
    </div>
  </div>`;
  document.getElementById('di-pull-back').onclick=()=>renderDockerImages(dc);
  document.getElementById('di-pull-go').onclick=async function(){
    const img=document.getElementById('di-pull-img').value.trim();
    const err=document.getElementById('di-pull-err');const out=document.getElementById('di-pull-out');
    if(!img){err.textContent='Image required';err.style.display='block';return}
    err.style.display='none';out.style.display='none';
    this.disabled=true;this.textContent='Pulling...';
    try{
      const r=await api('/docker/images/pull',{method:'POST',body:JSON.stringify({image:img})});
      if(r.success){out.style.display='block';out.textContent=r.data?.output||'Done';showToast('Pulled: '+img,'success');this.textContent='Done'}
      else{err.textContent=r.message||'Failed';err.style.display='block';this.disabled=false;this.textContent='Pull'}
    }catch(e){if(e.message!=='Unauthorized'){err.textContent='Error: '+e.message;err.style.display='block'}this.disabled=false;this.textContent='Pull'}
  };
}

// ── Networks ──
async function renderDockerNetworks(dc){
  dc.innerHTML='<div style="text-align:center;padding:2rem;color:var(--text-dim)">Loading networks...</div>';
  try{
    const j=await api('/docker/networks');
    if(!j.success){dc.innerHTML='<div class="error-msg">Failed</div>';return}
    const list=j.data||[];if(!Array.isArray(list)){dc.innerHTML='<div class="error-msg">Invalid</div>';return}
    let html='<div class="flex justify-between items-center mb-2"><span style="font-size:0.85rem;color:var(--text-dim)">'+list.length+' network(s)</span><button class="btn btn-sm btn-success" id="dn-add">+ Create</button></div>';
    html+='<div class="card"><div class="table-wrap"><table><thead><tr><th>Name</th><th>Driver</th><th>Scope</th><th>Subnet</th><th>Gateway</th><th>Actions</th></tr></thead><tbody>';
    list.forEach(n=>{
      html+=`<tr><td><strong>${escapeHtml(n.name||'')}</strong></td><td style="color:var(--text-dim)">${escapeHtml(n.driver||'')}</td><td style="font-size:0.8rem;color:var(--text-muted)">${escapeHtml(n.scope||'')}</td><td style="font-size:0.8rem;font-family:monospace">${escapeHtml(n.subnet||'—')}</td><td style="font-size:0.8rem;font-family:monospace">${escapeHtml(n.gateway||'—')}</td><td><button class="btn btn-sm btn-danger dn-rm" data-id="${n.id||''}">✕</button></td></tr>`;
    });
    html+='</tbody></table></div></div>';
    dc.innerHTML=html;
    document.getElementById('dn-add').onclick=()=>renderDockerNetworkCreate(dc);
    dc.querySelectorAll('.dn-rm').forEach(btn=>{btn.onclick=async function(){
      const id=this.dataset.id;if(!confirm('Remove?'))return;
      this.disabled=true;this.textContent='...';
      try{const r=await api('/docker/networks/'+encodeURIComponent(id)+'/remove',{method:'POST'});if(r.success){showToast('Removed','success');renderDockerNetworks(dc)}else showToast(r.message||'Failed','error')}catch(e){if(e.message!=='Unauthorized')showToast('Error: '+e.message,'error')}
    };});
  }catch(e){if(e.message!=='Unauthorized')dc.innerHTML='<div class="error-msg">Error: '+e.message+'</div>'}
}

async function renderDockerNetworkCreate(dc){
  dc.innerHTML=`
  <div class="card" style="max-width:500px">
    <h3 style="font-size:0.95rem;margin-bottom:1rem">Create Network</h3>
    <div class="field"><label>Name</label><input type="text" id="dn-name" placeholder="my-network"></div>
    <div class="field"><label>Driver</label><select id="dn-driver"><option value="bridge">bridge</option><option value="overlay">overlay</option><option value="macvlan">macvlan</option><option value="host">host</option></select></div>
    <div class="field"><label>Subnet</label><input type="text" id="dn-sub" placeholder="172.20.0.0/16"></div>
    <div class="field"><label>Gateway</label><input type="text" id="dn-gw" placeholder="172.20.0.1"></div>
    <div id="dn-err" class="error-msg" style="display:none"></div>
    <div class="btn-row mt-3">
      <button class="btn btn-sm" id="dn-c-back">Back</button>
      <button class="btn btn-sm btn-success" id="dn-c-do">Create</button>
    </div>
  </div>`;
  document.getElementById('dn-c-back').onclick=()=>renderDockerNetworks(dc);
  document.getElementById('dn-c-do').onclick=async function(){
    const nm=document.getElementById('dn-name').value.trim();
    const drv=document.getElementById('dn-driver').value;
    const sub=document.getElementById('dn-sub').value.trim();
    const gw=document.getElementById('dn-gw').value.trim();
    const err=document.getElementById('dn-err');
    if(!nm){err.textContent='Name required';err.style.display='block';return}
    err.style.display='none';this.disabled=true;this.textContent='Creating...';
    try{
      const body={name:nm,driver:drv};if(sub)body.subnet=sub;if(gw)body.gateway=gw;
      const r=await api('/docker/networks',{method:'POST',body:JSON.stringify(body)});
      if(r.success){showToast('Created','success');renderDockerNetworks(dc)}
      else{err.textContent=r.message||'Failed';err.style.display='block';this.disabled=false;this.textContent='Create'}
    }catch(e){if(e.message!=='Unauthorized'){err.textContent='Error: '+e.message;err.style.display='block'}this.disabled=false;this.textContent='Create'}
  };
}

// ── Volumes ──
async function renderDockerVolumes(dc){
  dc.innerHTML='<div style="text-align:center;padding:2rem;color:var(--text-dim)">Loading volumes...</div>';
  try{
    const j=await api('/docker/volumes');
    if(!j.success){dc.innerHTML='<div class="error-msg">Failed</div>';return}
    const list=j.data||[];if(!Array.isArray(list)){dc.innerHTML='<div class="error-msg">Invalid</div>';return}
    let html='<div class="flex justify-between items-center mb-2"><span style="font-size:0.85rem;color:var(--text-dim)">'+list.length+' volume(s)</span><button class="btn btn-sm btn-success" id="dv-add">+ Create</button></div>';
    html+='<div class="card"><div class="table-wrap"><table><thead><tr><th>Name</th><th>Driver</th><th>Mountpoint</th><th>Scope</th><th>Actions</th></tr></thead><tbody>';
    list.forEach(v=>{
      const nm=v.name||'';
      const mp=v.mountpoint||'';
      html+=`<tr><td><strong>${escapeHtml(nm.length>35?nm.substring(0,35)+'…':nm)}</strong></td><td style="color:var(--text-dim)">${escapeHtml(v.driver||'')}</td><td style="font-size:0.8rem;font-family:monospace;color:var(--text-muted);max-width:200px;overflow:hidden;text-overflow:ellipsis">${escapeHtml(mp)}</td><td style="font-size:0.8rem">${escapeHtml(v.scope||'')}</td><td><button class="btn btn-sm btn-danger dv-rm" data-name="${escapeHtml(nm)}">✕</button></td></tr>`;
    });
    html+='</tbody></table></div></div>';
    dc.innerHTML=html;
    document.getElementById('dv-add').onclick=()=>renderDockerVolumeCreate(dc);
    dc.querySelectorAll('.dv-rm').forEach(btn=>{btn.onclick=async function(){
      const n=this.dataset.name;if(!confirm('Remove volume '+n+'?'))return;
      this.disabled=true;this.textContent='...';
      try{const r=await api('/docker/volumes/'+encodeURIComponent(n)+'/remove',{method:'POST'});if(r.success){showToast('Removed','success');renderDockerVolumes(dc)}else showToast(r.message||'Failed','error')}catch(e){if(e.message!=='Unauthorized')showToast('Error: '+e.message,'error')}
    };});
  }catch(e){if(e.message!=='Unauthorized')dc.innerHTML='<div class="error-msg">Error: '+e.message+'</div>'}
}
async function renderDockerVolumeCreate(dc){
  dc.innerHTML=`
  <div class="card" style="max-width:450px">
    <h3 style="font-size:0.95rem;margin-bottom:1rem">Create Volume</h3>
    <div class="field"><label>Name</label><input type="text" id="dv-name" placeholder="my-volume"></div>
    <div class="field"><label>Driver</label><select id="dv-driver"><option value="local">local</option></select></div>
    <div id="dv-err" class="error-msg" style="display:none"></div>
    <div class="btn-row mt-3">
      <button class="btn btn-sm" id="dv-c-back">Back</button>
      <button class="btn btn-sm btn-success" id="dv-c-do">Create</button>
    </div>
  </div>`;
  document.getElementById('dv-c-back').onclick=()=>renderDockerVolumes(dc);
  document.getElementById('dv-c-do').onclick=async function(){
    const nm=document.getElementById('dv-name').value.trim();
    const drv=document.getElementById('dv-driver').value;
    const err=document.getElementById('dv-err');
    if(!nm){err.textContent='Name required';err.style.display='block';return}
    err.style.display='none';this.disabled=true;this.textContent='Creating...';
    try{
      const r=await api('/docker/volumes',{method:'POST',body:JSON.stringify({name:nm,driver:drv})});
      if(r.success){showToast('Created','success');renderDockerVolumes(dc)}
      else{err.textContent=r.message||'Failed';err.style.display='block';this.disabled=false;this.textContent='Create'}
    }catch(e){if(e.message!=='Unauthorized'){err.textContent='Error: '+e.message;err.style.display='block'}this.disabled=false;this.textContent='Create'}
  };
}
// ── System Info ──
async function renderSystemInfo(){
  const c=document.getElementById('content');
  if(!c)return;
  c.innerHTML='<div style="text-align:center;padding:2rem;color:var(--text-dim)">Loading system information...</div>';
  try{
    const j=await api('/os_info');
    if(!j.success||!j.data){c.innerHTML='<div class="error-msg">Failed to load system info</div>';return}
    const d=j.data;
    const os=d.os||{};
    const rows=[
      ['OS Name',os.name||'N/A'],
      ['Kernel Version',os.kernel_version||'N/A'],
      ['OS Version',os.os_version||'N/A'],
      ['Platform',os.long_os_version||os.os_type||'N/A'],
      ['Hostname',os.host_name||os.hostname||'N/A'],
      ['Architecture',os.architecture||'N/A'],
      ['Boot Time',os.boot_time?new Date((os.boot_time)*1000).toLocaleString():'N/A'],
      ['Updated At',d.updated_at?new Date(d.updated_at*1000).toLocaleString():'N/A']
    ];
    c.innerHTML=`<div class="card"><div class="table-wrap"><table class="sys-table"><tbody>
      ${rows.map(r=>`<tr><td>${r[0]}</td><td>${escapeHtml(r[1])}</td></tr>`).join('')}
    </tbody></table></div></div>`;
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
  </div>
  <div class="search-bar">
    <input type="text" id="process-search-input" placeholder="Search by name..." value="${processSearchKeyword||''}">
    <button class="btn btn-sm" id="process-search-btn">Search</button>
  </div>
  <div class="card" id="process-list-container">
    <div style="text-align:center;padding:2rem;color:var(--text-dim)">Loading processes...</div>
  </div>`;

  document.getElementById('process-search-btn').addEventListener('click',()=>{
    processSearchKeyword=document.getElementById('process-search-input').value.trim();
    loadProcessList();
  });
  document.getElementById('process-search-input').addEventListener('keydown',e=>{
    if(e.key==='Enter')document.getElementById('process-search-btn').click();
  });

  await loadProcessList();
  processInterval=setInterval(loadProcessList,5000);
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
    let html='<div class="table-wrap"><table><thead><tr><th>PID</th><th>Name</th><th>CPU%</th><th>Memory</th><th>Status</th><th>Run Time</th><th>Actions</th></tr></thead><tbody>';
    processes.forEach(p=>{
      const memStr=p.memory!=null?p.memory+' MB':'N/A';
      const cpuStr=p.cpu!=null?p.cpu+'%':'N/A';
      html+=`<tr>
        <td style="font-family:monospace">${p.pid||''}</td>
        <td>${escapeHtml(p.name||'')}</td>
        <td>${cpuStr}</td>
        <td>${memStr}</td>
        <td>${escapeHtml(p.status||'')}</td>
        <td style="color:var(--text-dim);font-size:0.8rem">${escapeHtml(p.run_time||p.runtime||'')}</td>
        <td><button class="btn btn-sm btn-danger kill-process-btn" data-pid="${p.pid}" data-name="${escapeHtml(p.name||'')}">Kill</button></td>
      </tr>`;
    });
    html+='</tbody></table></div>';
    container.innerHTML=html;
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
      logRefreshInterval=setInterval(()=>loadLogContent(currentLogFile),5000);
    } else {
      if(logRefreshInterval)clearInterval(logRefreshInterval);
      logRefreshInterval=null;
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

async function loadLogContent(filePath){
  const area=document.getElementById('log-content-area');
  if(!area)return;
  area.textContent='Loading...';
  try{
    let url='/log/read?path='+encodeURIComponent(filePath)+'&lines=200';
    if(currentLogKeyword)url+='&keyword='+encodeURIComponent(currentLogKeyword);
    const j=await api(url);
    if(!j.success){area.textContent=j.message||'Failed to load log';return}
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
  }catch(e){
    if(e.message!=='Unauthorized')area.textContent='Error: '+e.message;
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

async function renderWebServerSites(tc){
  tc.innerHTML='<div style="text-align:center;padding:2rem;color:var(--text-dim)">Loading sites...</div>';
  try{
    const j=await api('/webserver/sites');
    if(!j.success){tc.innerHTML='<div class="error-msg">Failed to load sites</div>';return}
    const sites=j.data||[];
    if(!Array.isArray(sites)){tc.innerHTML='<div class="error-msg">Invalid site data</div>';return}
    let html='<div class="card"><div class="table-wrap"><table><thead><tr><th>Server Name</th><th>Listen</th><th>Proxy / Root</th><th>SSL</th><th>Status</th><th>Actions</th></tr></thead><tbody>';
    sites.forEach(s=>{
      const displayName=s.server_name||s.name||'';
      const fileName=s.name||'';
      const listen=s.listen||'80';
      const proxy=s.proxy_pass||s.root||s.document_root||'';
      const enabled=s.enabled!=null?s.enabled:true;
      const ssl=s.ssl?'Yes':'No';
      html+=`<tr>
        <td>${escapeHtml(displayName)}</td>
        <td style="font-family:monospace;font-size:0.8rem">${escapeHtml(listen)}</td>
        <td style="color:var(--text-muted);font-size:0.8rem;max-width:250px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${escapeHtml(proxy)}</td>
        <td>${ssl==='Yes'?'🔒':''}</td>
        <td><span class="status-badge ${enabled?'running':'exited'}" id="status-${escapeHtml(fileName)}">${enabled?'Enabled':'Disabled'}</span></td>
        <td style="white-space:nowrap">
          <button class="btn btn-sm ${enabled?'btn-warning':'btn-success'} site-toggle-btn" data-name="${escapeHtml(fileName)}" data-enable="${enabled?'false':'true'}">${enabled?'Disable':'Enable'}</button>
          <button class="btn btn-sm btn-danger site-delete-btn" data-name="${escapeHtml(fileName)}">Delete</button>
        </td>
      </tr>`;
    });
    html+='</tbody></table></div></div>';
    tc.innerHTML=html;
    tc.querySelectorAll('.site-toggle-btn').forEach(btn=>{
      btn.addEventListener('click',async function(){
        const name=this.dataset.name;
        const enable=this.dataset.enable==='true';
        const action=enable?'enable':'disable';
        this.disabled=true;
        this.textContent='...';
        try{
          const r=await api('/webserver/sites/'+encodeURIComponent(name)+'/'+action,{method:'POST'});
          if(r.success){showToast('Site '+(enable?'enabled':'disabled'),'success');renderWebServerSites(tc)}
          else showToast(r.message||'Toggle failed','error');
        }catch(e){if(e.message!=='Unauthorized')showToast('Error: '+e.message,'error')}
      });
    });
    tc.querySelectorAll('.site-delete-btn').forEach(btn=>{
      btn.addEventListener('click',async function(){
        const name=this.dataset.name;
        const overlay=document.createElement('div');
        overlay.className='modal-overlay';
        overlay.innerHTML=`
        <div class="modal" style="max-width:380px">
          <h3>Confirm Delete</h3>
          <p style="color:var(--text-muted);font-size:0.9rem;margin-bottom:1rem">Delete site <strong>${escapeHtml(name)}</strong>?</p>
          <div class="btn-row">
            <button class="btn btn-sm" id="modal-cancel-btn">Cancel</button>
            <button class="btn btn-sm btn-danger" id="modal-confirm-btn">Delete</button>
          </div>
        </div>`;
        document.body.appendChild(overlay);
        function closeOverlay(){overlay.remove()}
        document.getElementById('modal-cancel-btn').addEventListener('click',closeOverlay);
        overlay.addEventListener('click',e=>{if(e.target===overlay)closeOverlay()});
        document.getElementById('modal-confirm-btn').addEventListener('click',async()=>{
          const confirmBtn=document.getElementById('modal-confirm-btn');
          confirmBtn.disabled=true;confirmBtn.textContent='Deleting...';
          try{
            const r=await api('/webserver/sites/'+encodeURIComponent(name),{method:'DELETE'});
            if(r.success){showToast('Site deleted','success');closeOverlay();renderWebServerSites(tc)}
            else showToast(r.message||'Delete failed','error');
          }catch(e){if(e.message!=='Unauthorized')showToast('Error: '+e.message,'error')}
          confirmBtn.disabled=false;confirmBtn.textContent='Delete';
        });
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
        <label>Root Directory</label>
        <input type="text" id="ws-root" placeholder="/var/www/example">
      </div>
    </div>
    <div class="form-row">
      <div class="field">
        <label>Proxy Pass (optional)</label>
        <input type="text" id="ws-proxy-pass" placeholder="http://127.0.0.1:3000">
      </div>
      <div class="field">
        <label>SSL</label>
        <select id="ws-ssl">
          <option value="">None</option>
          <option value="letsencrypt">Let's Encrypt</option>
          <option value="custom">Custom</option>
        </select>
      </div>
    </div>
    <div class="field">
      <label>Index Files</label>
      <input type="text" id="ws-index" placeholder="index.html index.htm" value="index.html index.htm">
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
      const body={server_name:name,root,proxy_pass:proxyPass,ssl,index,extra_config:customConfig};
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

    // Certificate table
    if(certs.length===0){
      html+='<div class="card"><div class="text-muted" style="text-align:center;padding:2rem">No certificates. Click "Issue" to create one.</div></div>';
    } else {
      html+='<div class="card"><div class="table-wrap"><table><thead><tr><th>Domain</th><th>Subject Alt Names</th><th>Expiry</th><th>Key Type</th><th>Actions</th></tr></thead><tbody>';
      certs.forEach(c=>{
        const domain=escapeHtml(c.domain||'?');
        const expiry=c.expiry||'Unknown';
        const expDate=new Date(expiry);
        const expStr=isNaN(expDate.getTime())?escapeHtml(expiry):expDate.toLocaleString();
        const expiringSoon=!isNaN(expDate.getTime())&&(expDate-Date.now())<30*24*60*60*1000;
        const sans=(c.san||[]).filter(s=>s!==c.domain).map(s=>escapeHtml(s)).join(', ');
        const keyType=c.key_type||'';
        html+=`<tr>
          <td><strong>${domain}</strong></td>
          <td style="font-size:0.8rem;color:var(--text-muted);max-width:200px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title="${escapeHtml(sans)}">${sans||'—'}</td>
          <td style="color:${expiringSoon?'var(--error)':'var(--text)'};white-space:nowrap">${expStr}</td>
          <td style="font-size:0.8rem;color:var(--text-dim)">${keyType?escapeHtml(keyType):'—'}</td>
          <td style="white-space:nowrap">
            <button class="btn btn-sm ssl-renew-btn" data-domain="${domain}">Renew</button>
            <button class="btn btn-sm ssl-deploy-btn" data-domain="${domain}">Deploy</button>
            <button class="btn btn-sm btn-danger ssl-delete-btn" data-domain="${domain}">✕</button>
          </td>
        </tr>`;
      });
      html+='</tbody></table></div></div>';
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
        const siteName=prompt('Deploy certificate to which nginx site? (site config filename)',domain);
        if(!siteName)return;
        this.disabled=true;this.textContent='...';
        try{
          const r=await api('/ssl/deploy',{method:'POST',body:JSON.stringify({site_name:siteName,domain})});
          if(r.success) showToast('Deployed to '+siteName,'success');
          else showToast(r.message||'Deploy failed','error');
        }catch(e){if(e.message!=='Unauthorized')showToast('Error: '+e.message,'error')}
        renderWebServerSsl(tc);
      });
    });

    // Delete
    tc.querySelectorAll('.ssl-delete-btn').forEach(btn=>{
      btn.addEventListener('click',async function(){
        const domain=this.dataset.domain;
        if(!confirm('Delete certificate for '+domain+'?'))return;
        this.disabled=true;this.textContent='...';
        try{
          const r=await api('/ssl/certificates/'+encodeURIComponent(domain),{method:'DELETE'});
          if(r.success){showToast('Deleted','success');renderWebServerSsl(tc)}
          else showToast(r.message||'Delete failed','error');
        }catch(e){if(e.message!=='Unauthorized')showToast('Error: '+e.message,'error')}
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
        resEl.style.display='block';
        resEl.innerHTML='<span style="color:var(--success)">✓ Certificate issued!</span><pre style="margin-top:8px;background:var(--bg);padding:8px;border-radius:4px;font-size:0.8rem;max-height:200px;overflow-y:auto">'+escapeHtml(r.data?.output||'')+'</pre>';
        btn.textContent='Done';
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

    const active=statusData.active!=null?statusData.active:statusData.enabled;

    let html=`
    <div class="flex justify-between items-center mb-4">
      <h2 style="font-size:1.1rem;font-weight:600">Firewall</h2>
      <div class="flex items-center gap-3">
        <span class="text-sm text-muted">Status:</span>
        <span class="status-badge ${active?'running':'exited'}">${active?'Active':'Inactive'}</span>
      </div>
    </div>`;

    html+=`<div class="card mb-4">
      <h3 style="font-size:0.95rem;margin-bottom:.75rem;color:var(--text-muted)">Rules</h3>
      <div class="table-wrap"><table>
        <thead><tr><th>Chain</th><th>#</th><th>Target</th><th>Prot</th><th>Source</th><th>Destination</th></tr></thead>
        <tbody>`;
    if(rules.length===0){
      html+=`<tr><td colspan="6" class="text-center text-muted">No rules</td></tr>`;
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

    document.getElementById('fw-del-rule-btn').addEventListener('click',async()=>{
      const chain=document.getElementById('fw-del-chain').value;
      const num=document.getElementById('fw-del-num').value;
      const errEl=document.getElementById('fw-del-error');
      if(!num){errEl.textContent='Rule number is required';errEl.style.display='block';return}
      errEl.style.display='none';
      const btn=document.getElementById('fw-del-rule-btn');
      btn.disabled=true;btn.textContent='Deleting...';
      try{
        const r=await api('/firewall/rules/'+encodeURIComponent(chain)+'/'+num,{
          method:'DELETE'
        });
        if(r.success){
          showToast('Rule deleted','success');
          renderFirewall();
        } else {
          errEl.textContent=r.message||'Failed to delete rule';
          errEl.style.display='block';
          btn.disabled=false;btn.textContent='Delete Rule';
        }
      }catch(e){
        if(e.message!=='Unauthorized'){
          errEl.textContent='Error: '+e.message;
          errEl.style.display='block';
          btn.disabled=false;btn.textContent='Delete Rule';
        }
      }
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
        const r=await api('/firewall/port',{
          method:'POST',
          body:JSON.stringify({port:parseInt(port),protocol:prot})
        });
        if(r.success){
          showToast('Port opened','success');
          renderFirewall();
        } else {
          errEl.textContent=r.message||'Failed to open port';
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
async function renderInstaller(){
  const c=document.getElementById('content');
  if(!c)return;
  c.innerHTML=`
  <div class="flex justify-between items-center mb-4">
    <h2 style="font-size:1.1rem;font-weight:600">Software Installer</h2>
  </div>
  <div id="installer-list" class="card">
    <div style="text-align:center;padding:2rem;color:var(--text-dim)">Loading available software...</div>
  </div>`;
  try{
    const j=await api('/installer/list');
    if(!j.success||!Array.isArray(j.data)){document.getElementById('installer-list').innerHTML='<div class="error-msg">Failed to load software list</div>';return}
    let html='<div class="table-wrap"><table><thead><tr><th>Software</th><th>Description</th><th>Actions</th></tr></thead><tbody>';
    j.data.forEach(s=>{
      html+=`<tr>
        <td><strong>${escapeHtml(s.name)}</strong></td>
        <td style="color:var(--text-muted)">${escapeHtml(s.description)}</td>
        <td>
          <button class="btn btn-sm btn-success inst-install" data-soft="${escapeHtml(s.name)}">Install</button>
          <button class="btn btn-sm btn-danger inst-uninstall" data-soft="${escapeHtml(s.name)}">Uninstall</button>
        </td>
      </tr>`;
    });
    html+='</tbody></table></div>';
    document.getElementById('installer-list').innerHTML=html;
    document.querySelectorAll('.inst-install').forEach(btn=>{
      btn.addEventListener('click',async function(){
        const soft=this.dataset.soft;
        const version=prompt('Version (leave empty for latest):')||'';
        this.disabled=true;this.textContent='Installing...';
        try{
          const r=await api('/installer/install',{method:'POST',body:JSON.stringify({software:soft,version:version||undefined})});
          if(r.success)showToast(soft+' installed successfully','success');
          else showToast(r.message||'Install failed','error');
        }catch(e){if(e.message!=='Unauthorized')showToast('Error: '+e.message,'error')}
        this.disabled=false;this.textContent='Install';
      });
    });
    document.querySelectorAll('.inst-uninstall').forEach(btn=>{
      btn.addEventListener('click',async function(){
        const soft=this.dataset.soft;
        if(!confirm('Uninstall '+soft+'?'))return;
        this.disabled=true;this.textContent='Uninstalling...';
        try{
          const r=await api('/installer/uninstall',{method:'POST',body:JSON.stringify({software:soft})});
          if(r.success)showToast(soft+' uninstalled successfully','success');
          else showToast(r.message||'Uninstall failed','error');
        }catch(e){if(e.message!=='Unauthorized')showToast('Error: '+e.message,'error')}
        this.disabled=false;this.textContent='Uninstall';
      });
    });
  }catch(e){
    if(e.message!=='Unauthorized')document.getElementById('installer-list').innerHTML='<div class="error-msg">Error: '+e.message+'</div>';
  }

}
