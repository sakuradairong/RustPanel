// ── Render ──
const $app=document.getElementById('app');

function render(route){
  cleanupPage();
  const token=getToken();
  if(route==='/login')return renderLogin();
  if(!token)return navigate('/login');
  if(route.startsWith('/file/view'))return layout(renderFileContent,route);
  if(route==='/home')return layout(renderDashboard,route);
  if(route==='/file/list')return layout(renderFileList,route);
  if(route==='/docker/containers')return layout(renderDocker,route);
  if(route==='/system/info')return layout(renderSystemInfo,route);
  if(route==='/admin/users')return layout(renderUserManagement,route);
  if(route==='/process')return layout(renderProcessManagement,route);
  if(route==='/services')return layout(renderServiceManagement,route);
  if(route==='/logs')return layout(renderLogViewer,route);
  if(route==='/monitor')return layout(renderMonitor,route);
  if(route==='/webserver')return layout(renderWebServer,route);
  if(route==='/firewall')return layout(renderFirewall,route);
  if(route==='/installer'||route==='/appstore'||route==='/apps')return layout(renderInstaller,route);
  layout(renderDashboard,route);
}

function layout(pageFn,route){
  const token=getToken();
  if(!token){navigate('/login');return}
  $app.innerHTML='<div id="sidebar"></div><div id="sidebar-overlay"></div><div id="main"><div id="topbar"></div><div id="content"></div></div>';
  renderSidebar(route);
  renderTopbar(route);
  document.getElementById('sidebar-overlay')?.addEventListener('click',closeSidebar);
  pageFn(route);
}

// Collapse the mobile sidebar + its overlay (no-op on desktop where it's static).
function closeSidebar(){
  document.getElementById('sidebar')?.classList.remove('open');
  document.getElementById('sidebar-overlay')?.classList.remove('show');
}

// ── Sidebar ──
let cachedMenus=null;
async function loadMenus(){
  if(cachedMenus)return cachedMenus;
  try{
    const j=await api('/user/menus');
    const menus = (j.data && j.data.menus) ? j.data.menus : (j.menus || j.data || []);
    if(Array.isArray(menus))cachedMenus=menus;
    else cachedMenus=[];
  }catch(e){cachedMenus=[]}
  return cachedMenus;
}

function routeFromMenuPath(p){
  if(!p||p==='/')return '/home';
  return p;
}

function renderSidebar(activeRoute){
  const sb=document.getElementById('sidebar');
  if(!sb)return;
  const userInfo=JSON.parse(localStorage.getItem('user')||'{}');
  sb.innerHTML='<div class="logo">RustPanel<span>Server Management</span></div><div class="nav" id="sidebar-nav"><div class="nav-loading" style="padding:1rem;color:var(--text-dim);font-size:0.85rem;text-align:center">Loading...</div></div>';
  loadMenus().then(menus=>{
    const nav=document.getElementById('sidebar-nav');
    if(!nav)return;
    let html='';
    if(menus.length===0){
      html+=`<div class="nav-item${activeRoute==='/home'?' active':''}" data-route="/home"><span class="icon">⌂</span> Dashboard</div>`;
      html+=`<div class="nav-item${activeRoute==='/file/list'?' active':''}" data-route="/file/list"><span class="icon">📁</span> Files</div>`;
      html+=`<div class="nav-item${activeRoute==='/docker/containers'?' active':''}" data-route="/docker/containers"><span class="icon">🐳</span> Docker</div>`;
      html+=`<div class="nav-item${activeRoute==='/system/info'?' active':''}" data-route="/system/info"><span class="icon">ℹ</span> System Info</div>`;
      html+=`<div class="nav-item${activeRoute==='/admin/users'?' active':''}" data-route="/admin/users"><span class="icon">👥</span> Users</div>`;
      html+=`<div class="nav-item${activeRoute==='/process'?' active':''}" data-route="/process"><span class="icon">⚡</span> Processes</div>`;
      html+=`<div class="nav-item${activeRoute==='/services'?' active':''}" data-route="/services"><span class="icon">🔧</span> Services</div>`;
      html+=`<div class="nav-item${activeRoute==='/logs'?' active':''}" data-route="/logs"><span class="icon">📋</span> Logs</div>`;
      html+=`<div class="nav-item${activeRoute==='/monitor'?' active':''}" data-route="/monitor"><span class="icon">📊</span> Monitor</div>`;
      html+=`<div class="nav-item${activeRoute==='/webserver'?' active':''}" data-route="/webserver"><span class="icon">🌐</span> Web Server</div>`;
      html+=`<div class="nav-item${activeRoute==='/firewall'?' active':''}" data-route="/firewall"><span class="icon">🛡</span> Firewall</div>`;
      html+=`<div class="nav-item${activeRoute==='/installer'?' active':''}" data-route="/installer"><span class="icon">📦</span> Software</div>`;
    } else {
      menus.forEach(item=>{
        const r=routeFromMenuPath(item.path);
        const hasChildren=item.children&&item.children.length>0;
        if(hasChildren){
          const open=item.children.some(c=>activeRoute===routeFromMenuPath(c.path));
          html+=`<div class="nav-group">`;
          html+=`<div class="group-title" data-toggle="${item.key||''}"><span class="icon">${getMenuIcon(item.icon)}</span>${item.name}<span class="arrow${open?' open':''}">▶</span></div>`;
          html+=`<div class="sub-items${open?' open':''}" data-group="${item.key||''}">`;
          item.children.forEach(child=>{
            const cr=routeFromMenuPath(child.path);
            html+=`<div class="nav-item${activeRoute===cr?' active':''}" data-route="${cr}"><span class="icon">${getMenuIcon(child.icon)}</span>${child.name}</div>`;
          });
          html+=`</div></div>`;
        } else {
          html+=`<div class="nav-item${activeRoute===r?' active':''}" data-route="${r}"><span class="icon">${getMenuIcon(item.icon)}</span>${item.name}</div>`;
        }
      });
    }
    nav.innerHTML=html;
    sb.querySelectorAll('.nav-item[data-route]').forEach(el=>{
      el.addEventListener('click',()=>{navigate(el.dataset.route);closeSidebar()});
    });
    sb.querySelectorAll('.group-title').forEach(el=>{
      el.addEventListener('click',function(){
        const sub=this.parentElement.querySelector('.sub-items');
        const arrow=this.querySelector('.arrow');
        if(sub){
          const isOpen=sub.classList.toggle('open');
          if(arrow)arrow.classList.toggle('open',isOpen);
        }
      });
    });
  }).catch(()=>{
    const nav=document.getElementById('sidebar-nav');
    if(nav)nav.innerHTML='<div style="padding:1rem;color:var(--text-dim);font-size:0.85rem;text-align:center">Menu load failed</div>';
  });
}

function getMenuIcon(icon){
  if(!icon)return '▫';
  const name=String(icon).toLowerCase();
  if(name.includes('home'))return '⌂';
  if(name.includes('file'))return '📁';
  if(name.includes('global')||name.includes('web'))return '🌐';
  if(name.includes('setting'))return '⚙';
  if(name.includes('pie')||name.includes('chart')||name.includes('data'))return '📊';
  if(name.includes('team')||name.includes('user'))return '👥';
  if(name.includes('idcard')||name.includes('admin'))return '🛡';
  if(name.includes('rocket')||name.includes('online'))return '🚀';
  if(name.includes('docker')||name.includes('container'))return '🐳';
  if(name.includes('info')||name.includes('system'))return 'ℹ';
  return '▫';
}

// ── Topbar ──
function renderTopbar(route){
  const tb=document.getElementById('topbar');
  if(!tb)return;
  const userInfo=JSON.parse(localStorage.getItem('user')||'{}');
  const pageNames={'/home':'Dashboard','/file/list':'File Manager','/docker/containers':'Docker Containers','/system/info':'System Information','/admin/users':'User Management','/process':'Process Management','/services':'Service Management','/logs':'Log Viewer','/monitor':'Real-Time Monitor','/webserver':'Web Server','/firewall':'Firewall','/installer':'Software Installer'};
  const baseRoute=route.split('?')[0];
  let title=pageNames[baseRoute]||'RustPanel';
  if(baseRoute.startsWith('/file/view'))title='File Viewer';
  const uname=userInfo.username||'User';
  tb.innerHTML=`<button id="sidebar-toggle" aria-label="Menu">☰</button><span class="page-title">${escapeHtml(title)}</span><div class="user-info"><span class="avatar" aria-hidden="true">${escapeHtml(initials(uname))}</span><span class="username">${escapeHtml(uname)}</span><button class="logout-btn" id="logout-btn"><span class="icon" aria-hidden="true">⎋</span>Logout</button></div>`;
  document.getElementById('logout-btn')?.addEventListener('click',()=>{
    confirmDialog({
      title:'Sign Out',
      message:'Are you sure you want to sign out?',
      okText:'Sign Out',okClass:'btn-danger',
      onConfirm:()=>{clearToken();cachedMenus=null;navigate('/login')}
    });
  });
  document.getElementById('sidebar-toggle')?.addEventListener('click',()=>{
    const sb=document.getElementById('sidebar');
    if(sb)sb.classList.toggle('open');
    const ov=document.getElementById('sidebar-overlay');
    if(ov)ov.classList.toggle('show');
  });
}

// ── Login ──
function renderLogin(){
  $app.innerHTML=`
  <div id="login-page">
    <div class="login-card">
      <h1>RustPanel</h1>
      <p class="subtitle">Server Management Panel</p>
      <div class="login-error" id="login-error"></div>
      <div class="field">
        <label for="login-username">Username</label>
        <input type="text" id="login-username" autocomplete="username" placeholder="Enter username">
      </div>
      <div class="field">
        <label for="login-password">Password</label>
        <div class="pw-field">
          <input type="password" id="login-password" autocomplete="current-password" placeholder="Enter password">
          <button type="button" class="pw-toggle" id="login-pw-toggle" aria-label="Show password">👁</button>
        </div>
      </div>
      <button class="btn login-btn" id="login-btn">Sign In</button>
      <div class="loading-bar" id="login-loading"></div>
    </div>
  </div>`;
  const params=getHashParams();
  const vParam=params.v||SECURITY_DIR;

  const errEl=document.getElementById('login-error');
  const btn=document.getElementById('login-btn');
  const unEl=document.getElementById('login-username');
  const pwEl=document.getElementById('login-password');
  const pwToggle=document.getElementById('login-pw-toggle');
  const loadingEl=document.getElementById('login-loading');

  function showErr(msg){errEl.textContent=msg;errEl.classList.add('show')}
  function hideErr(){errEl.classList.remove('show')}

  // Show/hide password
  pwToggle?.addEventListener('click',()=>{
    const reveal=pwEl.type==='password';
    pwEl.type=reveal?'text':'password';
    pwToggle.textContent=reveal?'🙈':'👁';
    pwToggle.setAttribute('aria-label',reveal?'Hide password':'Show password');
    pwEl.focus();
  });

  // Remember the last username and focus the most useful field.
  const lastUser=localStorage.getItem('lastUsername')||'';
  if(lastUser){unEl.value=lastUser;setTimeout(()=>pwEl.focus(),0)}
  else setTimeout(()=>unEl.focus(),0);

  async function doLogin(){
    hideErr();
    const username=unEl.value.trim();
    const password=pwEl.value;
    if(!username||!password){showErr('Please enter username and password');return}
    btn.disabled=true;btn.textContent='Signing in...';loadingEl.classList.add('show');
    try{
      const ts=Date.now();
      const iv=md5(vParam);
      const keyWithToken=vParam+String(ts)+iv;
      const encKey=md5(keyWithToken);
      const encUsername=sm4_cbc_encrypt_hex(username,encKey,iv);
      const encPassword=sm4_cbc_encrypt_hex(password,encKey,iv);
      const j=await fetch(apiUrl('/login/sign/'+encodeURIComponent(vParam)),{
        method:'POST',
        headers:{'Content-Type':'application/json'},
        body:JSON.stringify({username:encUsername,password:encPassword,timestamp:ts})
      }).then(r=>r.json());
      if(j.success&&j.data&&j.data.token){
        setToken(j.data.token);
        if(j.data.user)localStorage.setItem('user',JSON.stringify(j.data.user));
        localStorage.setItem('lastUsername',username);
        cachedMenus=null;
        navigate('/home');
      } else {
        const msg=j.message||'Login failed';
        if(msg.includes('username_band')||msg.includes('band'))showErr('Account has been locked');
        else if(msg.includes('password_error'))showErr('Incorrect password');
        else if(msg.includes('username_error'))showErr('Username does not exist');
        else showErr(msg);
      }
    }catch(e){
      if(e.message==='Unauthorized')return;
      showErr('Connection error: '+e.message);
      showToast('Connection error: '+e.message,'error');
    }finally{
      btn.disabled=false;btn.textContent='Sign In';loadingEl.classList.remove('show');
    }
  }

  btn.addEventListener('click',doLogin);
  unEl.addEventListener('keydown',e=>{if(e.key==='Enter')doLogin()});
  pwEl.addEventListener('keydown',e=>{if(e.key==='Enter')doLogin()});
}

// ── Dashboard ──
let dashboardEventSource=null;

// ── 1Panel-style ring gauges ──
const GAUGE_C=314.159; // 2*pi*r, r=50
function gaugeCard(id,title){
  return `<div class="card gauge">
    <div class="gauge-title">${title}</div>
    <div class="gauge-ring">
      <svg viewBox="0 0 120 120"><circle class="gauge-track" cx="60" cy="60" r="50"></circle>
      <circle class="gauge-arc" id="${id}-arc" cx="60" cy="60" r="50" stroke-dasharray="${GAUGE_C}" stroke-dashoffset="${GAUGE_C}"></circle></svg>
      <div class="gauge-center" id="${id}-val">—</div>
    </div>
    <div class="gauge-sub" id="${id}-sub">&nbsp;</div>
  </div>`;
}
function gaugeColor(pct){return pct>85?'var(--error)':pct>60?'var(--warning)':'var(--success)'}
function setGauge(id,fillPct,color,centerText,sub){
  const p=Math.min(Math.max(fillPct,0),100);
  const arc=document.getElementById(id+'-arc');
  if(arc){arc.style.strokeDashoffset=(GAUGE_C*(1-p/100)).toFixed(2);arc.style.stroke=color}
  const val=document.getElementById(id+'-val');
  if(val){val.textContent=centerText;val.style.color=color}
  const subEl=document.getElementById(id+'-sub');
  if(subEl)subEl.textContent=sub;
}
function updateGauges(s,coreCount){
  const cpu=s.cpu||0;
  const memUsed=s.memory?s.memory.used:0,memTotal=s.memory?s.memory.total:1;
  const swUsed=s.swap?s.swap.used:0,swTotal=s.swap?s.swap.total:0;
  const memPct=memTotal?(memUsed/memTotal)*100:0;
  const swPct=swTotal?(swUsed/swTotal)*100:0;
  const l1=s.load?s.load.one:0,l5=s.load?s.load.five:0,l15=s.load?s.load.fifteen:0;
  const loadPct=coreCount?Math.min((l1/coreCount)*100,100):0;
  setGauge('g-cpu',cpu,gaugeColor(cpu),cpu.toFixed(0)+'%',(coreCount||0)+' cores');
  setGauge('g-mem',memPct,gaugeColor(memPct),memPct.toFixed(0)+'%',formatSize(memUsed)+' / '+formatSize(memTotal));
  setGauge('g-swap',swPct,gaugeColor(swPct),swPct.toFixed(0)+'%',swTotal?(formatSize(swUsed)+' / '+formatSize(swTotal)):'No swap');
  setGauge('g-load',loadPct,gaugeColor(loadPct),l1.toFixed(2),l5.toFixed(2)+' · '+l15.toFixed(2)+' (5m·15m)');
}
function formatUptime(sec){
  sec=Math.max(0,Math.floor(sec));
  const d=Math.floor(sec/86400),h=Math.floor((sec%86400)/3600),m=Math.floor((sec%3600)/60);
  if(d>0)return d+'d '+h+'h';
  if(h>0)return h+'h '+m+'m';
  return m+'m';
}

async function renderDashboard(){
  const c=document.getElementById('content');
  if(!c)return;
  cleanupPage();
  if(dashboardEventSource){dashboardEventSource.close();dashboardEventSource=null}

  c.innerHTML=`
  <div class="card ds-overview mb-4" id="ds-overview"><div class="skeleton skeleton-text" style="width:60%"></div></div>
  <div class="gauge-grid mb-4" id="ds-gauges">
    ${gaugeCard('g-cpu','CPU')}
    ${gaugeCard('g-mem','Memory')}
    ${gaugeCard('g-swap','Swap')}
    ${gaugeCard('g-load','Load')}
  </div>
  <div class="card mb-4"><h3 class="ds-h">Disk Usage</h3><div id="ds-disks"><div class="skeleton skeleton-text"></div><div class="skeleton skeleton-text"></div></div></div>
  <div class="card"><h3 class="ds-h">Network Interfaces</h3><div id="ds-network"><div class="skeleton skeleton-text"></div></div></div>`;

  try{
    const j=await api('/os_info');
    if(!j.success||!j.data){document.getElementById('ds-overview').textContent='Failed to load system info';return}
    const d=j.data;
    const os=d.os||{};
    const mem=d.memory||{};
    const cpus=d.cpu||[];
    const disks=d.disk||[];
    const nets=d.network||[];
    const load=d.load||{};
    const coreCount=cpus.length;

    // Overview strip: host / OS / kernel / arch / uptime
    const bootTime=Number(os.boot_time)||0;
    const nowSec=Number(d.updated_at)||Math.floor(Date.now()/1000);
    const uptime=bootTime>0?formatUptime(nowSec-bootTime):'—';
    const osName=(os.name||os.os_type||'Linux')+(os.os_version?(' '+os.os_version):'');
    document.getElementById('ds-overview').innerHTML=
      `<div class="ov-item"><span class="ov-k">Host</span><span class="ov-v">${escapeHtml(os.host_name||os.hostname||'—')}</span></div>`+
      `<div class="ov-item"><span class="ov-k">OS</span><span class="ov-v">${escapeHtml(osName)}</span></div>`+
      `<div class="ov-item"><span class="ov-k">Kernel</span><span class="ov-v">${escapeHtml(os.kernel_version||'—')}</span></div>`+
      `<div class="ov-item"><span class="ov-k">Arch</span><span class="ov-v">${escapeHtml(os.architecture||'—')}</span></div>`+
      `<div class="ov-item"><span class="ov-k">Uptime</span><span class="ov-v">${uptime}</span></div>`;

    // Initial gauge values from the os_info snapshot (SSE then keeps them live)
    const cpuAvg=coreCount?cpus.reduce((a,b)=>a+b,0)/coreCount:0;
    updateGauges({
      cpu:cpuAvg,
      memory:{used:(mem.used||0)*1048576,total:(mem.total||1)*1048576},
      swap:{used:(mem.swap_used||0)*1048576,total:(mem.swap_total||0)*1048576},
      load:{one:load.one||0,five:load.five||0,fifteen:load.fifteen||0}
    },coreCount);

    // Disk usage
    let diskHtml='';
    disks.forEach(dd=>{
      const total=dd.total||1;
      const avail=dd.available||0;
      const used=total-avail;
      const pct=((used/total)*100).toFixed(1);
      const color=pct>80?'red':pct>50?'orange':'green';
      diskHtml+=`<div class="disk-row"><span class="mount">${dd.mount_point||'?'}</span>
        <div class="progress-bar" style="flex:1;margin:0 12px"><div class="fill ${color}" style="width:${Math.min(pct,100)}%"></div></div>
        <span class="usage">${formatSize(used*1048576)} / ${formatSize(total*1048576)} (${pct}%)</span></div>`;
    });
    document.getElementById('ds-disks').innerHTML=diskHtml||'<div class="text-muted text-sm">No disk data</div>';

    // Network interfaces (live rates come from SSE deltas)
    if(nets.length>0){
      let netHtml='<div class="table-wrap"><table><thead><tr><th>Interface</th><th>Received</th><th>Transmitted</th></tr></thead><tbody>';
      nets.forEach(n=>{
        netHtml+=`<tr><td>${n.name||'?'}</td><td id="net-rx-${n.name}">${formatSize((n.total_received||0)*1024)}</td><td id="net-tx-${n.name}">${formatSize((n.total_transmitted||0)*1024)}</td></tr>`;
      });
      netHtml+='</tbody></table></div>';
      document.getElementById('ds-network').innerHTML=netHtml;
    } else {
      document.getElementById('ds-network').innerHTML='<div class="text-muted text-sm">No network data</div>';
    }

    let prevNet={};
    nets.forEach(n=>{prevNet[n.name]={rx:n.total_received||0,tx:n.total_transmitted||0}});

    // Live updates via SSE: refresh the gauges and per-interface rates.
    const token=getToken();
    dashboardEventSource = new EventSource('/api/v1/monitor?token='+encodeURIComponent(token));
    dashboardEventSource.onmessage=function(ev){
      try{
        const s=JSON.parse(ev.data);
        updateGauges(s,coreCount);
        if(s.network&&Array.isArray(s.network)){
          s.network.forEach(ni=>{
            const rxEl=document.getElementById('net-rx-'+ni.name);
            const txEl=document.getElementById('net-tx-'+ni.name);
            if(rxEl){
              const prev=prevNet[ni.name];
              if(prev){
                const rxDelta=ni.received-prev.rx;
                const txDelta=ni.transmitted-prev.tx;
                rxEl.textContent=(rxDelta>=0?formatSize(rxDelta):'?')+'/s';
                txEl.textContent=(txDelta>=0?formatSize(txDelta):'?')+'/s';
              }
              prevNet[ni.name]={rx:ni.received,tx:ni.transmitted};
            }
          });
        }
      }catch(e){/* ignore parse errors */ }
    };
    dashboardEventSource.onerror=function(){/* SSE connection will auto-reconnect */};
  }catch(e){
    if(e.message!=='Unauthorized'){const ov=document.getElementById('ds-overview');if(ov)ov.textContent='Error: '+e.message}
  }
}

function formatSize(bytes){
  if(bytes===0)return '0 B';
  if(bytes<0)return 'N/A';
  const units=['B','KB','MB','GB','TB'];
  const i=Math.floor(Math.log(bytes)/Math.log(1024));
  return (bytes/Math.pow(1024,i)).toFixed(i>0?1:0)+' '+units[i];
}

// ── File Manager ──
let fileListState={path:'/',page:1};

// Pick an icon for a file by extension.
function fileIcon(name){
  const ext=(String(name).split('.').pop()||'').toLowerCase();
  if(['png','jpg','jpeg','gif','webp','svg','bmp','ico'].indexOf(ext)>=0)return '🖼️';
  if(['zip','tar','gz','tgz','bz2','xz','rar','7z'].indexOf(ext)>=0)return '🗜️';
  if(['js','ts','jsx','tsx','rs','go','py','java','c','cpp','h','sh','rb','php','html','css','json','yaml','yml','toml','xml','md'].indexOf(ext)>=0)return '📜';
  if(['mp4','mkv','mov','avi','webm'].indexOf(ext)>=0)return '🎞️';
  if(['mp3','wav','flac','ogg'].indexOf(ext)>=0)return '🎵';
  if(ext==='pdf')return '📕';
  return '📄';
}

// Render a clickable breadcrumb for the given absolute path.
function renderFileBreadcrumb(path){
  const el=document.getElementById('file-breadcrumb');
  if(!el)return;
  const parts=String(path||'/').split('/').filter(Boolean);
  let acc='';
  let html='<span class="crumb" data-path="/">🏠 /</span>';
  parts.forEach(seg=>{
    acc+='/'+seg;
    html+='<span class="crumb-sep">/</span><span class="crumb" data-path="'+escapeHtml(acc)+'">'+escapeHtml(seg)+'</span>';
  });
  el.innerHTML=html;
  el.querySelectorAll('.crumb[data-path]').forEach(cr=>{
    cr.addEventListener('click',()=>{fileListState.path=cr.dataset.path;fileListState.page=1;loadFileList()});
  });
}

async function renderFileList(){
  const c=document.getElementById('content');
  if(!c)return;
  c.innerHTML=`
  <div id="file-breadcrumb" class="file-breadcrumb"></div>
  <div class="file-path">
    <input type="text" id="file-path-input" value="${fileListState.path}" placeholder="/">
    <button class="btn btn-sm" id="file-go-btn">Go</button>
  </div>
  <div class="card" id="file-list-container">
    <div style="text-align:center;padding:2rem;color:var(--text-dim)">Loading...</div>
  </div>`;

  document.getElementById('file-go-btn').addEventListener('click',()=>{
    fileListState.path=document.getElementById('file-path-input').value.trim()||'/';
    fileListState.page=1;
    loadFileList();
  });
  document.getElementById('file-path-input').addEventListener('keydown',e=>{
    if(e.key==='Enter')document.getElementById('file-go-btn').click();
  });

  await loadFileList();
}

async function loadFileList(){
  const container=document.getElementById('file-list-container');
  if(!container)return;
  container.innerHTML='<div style="text-align:center;padding:2rem;color:var(--text-dim)">Loading...</div>';
  try{
    const path=fileListState.path||'/';
    const j=await api('/file/list?path='+encodeURIComponent(path)+'&current='+fileListState.page+'&pageSize=50');
    if(!j.success){container.innerHTML='<div class="error-msg">Failed to load directory</div>';return}
    const d=j.data;
    const curPath=d.path||path;
    renderFileBreadcrumb(curPath);
    const inputEl=document.getElementById('file-path-input');
    if(inputEl)inputEl.value=curPath;
    let html='<div class="table-wrap"><table class="file-list-table"><thead><tr><th>Name</th><th>Size</th><th>Modified</th></tr></thead><tbody>';
    if(d.path&&d.path!==path){
      const parent=d.path.replace(/\/?[^\/]*\/?$/,'')||'/';
      html+=`<tr><td class="clickable" data-path="${escapeHtml(parent)}" data-type="dir">↩ .. (parent)</td><td></td><td></td></tr>`;
    }
    (d.dirs||[]).forEach(dir=>{
      const fullPath=(curPath.endsWith('/')?curPath:curPath+'/')+dir.name;
      html+=`<tr><td class="clickable" data-path="${escapeHtml(fullPath)}" data-type="dir">📁 ${escapeHtml(dir.name)}</td><td></td><td>${dir.modified_time?formatTime(dir.modified_time):''}</td></tr>`;
    });
    (d.files||[]).forEach(f=>{
      const fullPath=(curPath.endsWith('/')?curPath:curPath+'/')+f.name;
      html+=`<tr><td class="clickable" data-path="${escapeHtml(fullPath)}" data-type="file">${fileIcon(f.name)} ${escapeHtml(f.name)}</td><td>${f.size!=null?formatSize(f.size):''}</td><td>${f.modified_time?formatTime(f.modified_time):''}</td></tr>`;
    });
    html+='</tbody></table></div>';
    container.innerHTML=html;
    container.querySelectorAll('[data-path]').forEach(el=>{
      el.addEventListener('click',function(){
        const p=this.dataset.path;
        const type=this.dataset.type;
        if(type==='dir'){
          fileListState.path=p;
          fileListState.page=1;
          loadFileList();
        } else {
          navigate('/file/view',{path:p});
        }
      });
    });
  }catch(e){
    if(e.message!=='Unauthorized')container.innerHTML=`<div class="error-msg">Error: ${e.message}</div>`;
  }
}
