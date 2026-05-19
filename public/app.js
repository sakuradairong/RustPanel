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
  if(route==='/installer')return layout(renderInstaller,route);
  layout(renderDashboard,route);
}

function layout(pageFn,route){
  const token=getToken();
  if(!token){navigate('/login');return}
  $app.innerHTML='<div id="sidebar"></div><div id="sidebar-overlay"></div><div id="main"><div id="topbar"></div><div id="content"></div></div>';
  renderSidebar(route);
  renderTopbar(route);
  pageFn(route);
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
      el.addEventListener('click',()=>navigate(el.dataset.route));
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
  tb.innerHTML=`<button id="sidebar-toggle" aria-label="Menu">☰</button><span class="page-title">${title}</span><div class="user-info"><span class="username">${userInfo.username||'User'}</span><button class="logout-btn" id="logout-btn">Logout</button></div>`;
  document.getElementById('logout-btn')?.addEventListener('click',()=>{clearToken();cachedMenus=null;navigate('/login')});
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
        <input type="password" id="login-password" autocomplete="current-password" placeholder="Enter password">
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
  const loadingEl=document.getElementById('login-loading');

  function showErr(msg){errEl.textContent=msg;errEl.classList.add('show')}
  function hideErr(){errEl.classList.remove('show')}

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
    }finally{
      btn.disabled=false;btn.textContent='Sign In';loadingEl.classList.remove('show');
    }
  }

  btn.addEventListener('click',doLogin);
  pwEl.addEventListener('keydown',e=>{if(e.key==='Enter')doLogin()});
}

// ── Dashboard ──
let dashboardEventSource=null;
async function renderDashboard(){
  const c=document.getElementById('content');
  if(!c)return;
  cleanupPage();
  if(dashboardEventSource){dashboardEventSource.close();dashboardEventSource=null}

  // Skeletons rendersıs
  c.innerHTML=`
  <div class="card-grid card-grid-4 mb-4" id="ds-stats">
    <div class="card stat-card"><div class="label" style="font-size:.72rem;text-transform:uppercase">CPU Usage</div><div class="skeleton skeleton-card" style="height:45px;margin-top:8px"></div></div>
    <div class="card stat-card"><div class="label" style="font-size:.72rem;text-transform:uppercase">Memory</div><div class="skeleton skeleton-card" style="height:45px;margin-top:8px"></div></div>
    <div class="card stat-card"><div class="label" style="font-size:.72rem;text-transform:uppercase">Swap</div><div class="skeleton skeleton-card" style="height:45px;margin-top:8px"></div></div>
    <div class="card stat-card"><div class="label" style="font-size:.72rem;text-transform:uppercase">Kernel</div><div class="skeleton skeleton-card" style="height:45px;margin-top:8px"></div></div>
  </div>
  <div class="card-grid card-grid-2">
    <div class="card"><h3 class="label" style="font-size:.72rem;text-transform:uppercase;letter-spacing:.06em;font-weight:700;color:var(--text-dim);margin-bottom:.8rem">CPU Usage</h3><div id="ds-cpu-cores"><div class="skeleton skeleton-text"></div><div class="skeleton skeleton-text" style="width:50%"></div></div></div>
    <div class="card"><h3 class="label" style="font-size:.72rem;text-transform:uppercase;letter-spacing:.06em;font-weight:700;color:var(--text-dim);margin-bottom:.8rem">Memory Usage</h3><div id="ds-memory"><div class="skeleton skeleton-text"></div><div class="skeleton skeleton-text" style="width:50%"></div></div></div>
  </div>
  <div class="card mt-4"><h3 class="label" style="font-size:.72rem;text-transform:uppercase;letter-spacing:.06em;font-weight:700;color:var(--text-dim);margin-bottom:.8rem">Disk Usage</h3><div id="ds-disks"><div class="skeleton skeleton-text"></div><div class="skeleton skeleton-text"></div></div></div>
  <div class="card mt-4"><h3 class="label" style="font-size:.72rem;text-transform:uppercase;letter-spacing:.06em;font-weight:700;color:var(--text-dim);margin-bottom:.8rem">Network Interfaces</h3><div id="ds-network"><div class="skeleton skeleton-text"></div></div></div>`;

  try{
    // Initial load: static OS info + disk layout
    const j=await api('/os_info');
    if(!j.success||!j.data){c.querySelector('#ds-cpu-cores').textContent='Failed to load';return}
    const d=j.data;
    const os=d.os||{};
    const mem=d.memory||{};
    const cpus=d.cpu||[];
    const disks=d.disk||[];
    const nets=d.network||[];
    const coreCount=cpus.length;

    // Render static OS info
    document.getElementById('ds-stats').querySelectorAll('.card.stat-card')[3].innerHTML=
      '<div class="label">Kernel</div><div class="value" style="color:var(--text);font-size:0.9rem">'+(os.kernel_version||'N/A')+'</div><div class="sub-value">'+(os.host_name||os.hostname||'')+'</div>';

    // Disk (static - rarely changes)
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

    // Network (static - will use SSE for live rates)
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

    // Store initial network counters for delta calculation
    let prevNet={};
    nets.forEach(n=>{prevNet[n.name]={rx:n.total_received||0,tx:n.total_transmitted||0}});

    // Open SSE for live CPU/Memory/Swap/Network updates
    const token=getToken();
    dashboardEventSource = new EventSource('/api/v1/monitor?token='+encodeURIComponent(token));
    dashboardEventSource.onmessage=function(ev){
      try{
        const s=JSON.parse(ev.data);
        const cpuGlobal=s.cpu||0;
        const memUsed=s.memory?s.memory.used:0;
        const memTotal=s.memory?s.memory.total:1;
        const swUsed=s.swap?s.swap.used:0;
        const swTotal=s.swap?s.swap.total:1;
        const memPct=((memUsed/memTotal)*100).toFixed(1);
        const swPct=swTotal>0?((swUsed/swTotal)*100).toFixed(1):'0.0';
        const cpuColor=cpuGlobal>80?'var(--error)':cpuGlobal>50?'var(--warning)':'var(--success)';
        const memColor=memPct>80?'var(--error)':memPct>50?'var(--warning)':'var(--primary)';
        const swColor=swPct>80?'var(--error)':swPct>50?'var(--warning)':'var(--secondary)';

        // Update stat cards
        const stats=document.getElementById('ds-stats');
        if(stats){
          const cards=stats.querySelectorAll('.card.stat-card');
          cards[0].innerHTML='<div class="stat-card-value">'+cpuGlobal.toFixed(1)+'%</div><div class="stat-card-sub">'+coreCount+' cores</div>';
          cards[1].innerHTML='<div class="stat-card-value">'+memPct+'%</div><div class="stat-card-sub">'+formatSize(memUsed)+' / '+formatSize(memTotal)+'</div>';
          cards[2].innerHTML='<div class="stat-card-value">'+swPct+'%</div><div class="stat-card-sub">'+formatSize(swUsed)+' / '+formatSize(swTotal)+'</div>';
        }

        // CPU bar
        const cpuEl=document.getElementById('ds-cpu-cores');
        if(cpuEl){
          const pct=Math.min(cpuGlobal,100).toFixed(1);
          const color=pct>80?'red':pct>50?'orange':'green';
          cpuEl.innerHTML='<div class="cpu-item"><div class="cpu-label"><span>Total</span><span>'+pct+'%</span></div><div class="bar"><div class="fill '+color+'" style="width:'+pct+'%"></div></div></div>';
        }

        // Memory bar
        const memEl=document.getElementById('ds-memory');
        if(memEl){
          const memPctNum=Math.min(parseFloat(memPct),100);
          const mColor=memPctNum>80?'red':memPctNum>50?'orange':'blue';
          let html='<div class="text-sm" style="display:flex;justify-content:space-between;margin-bottom:4px"><span>RAM</span><span>'+formatSize(memUsed)+' / '+formatSize(memTotal)+'</span></div>';
          html+='<div class="progress-bar"><div class="fill '+mColor+'" style="width:'+memPctNum+'%"></div></div>';
          if(swTotal>0){
            const swPctNum=Math.min(parseFloat(swPct),100);
            const sColor=swPctNum>80?'red':swPctNum>50?'orange':'blue';
            html+='<div class="text-sm mt-3" style="display:flex;justify-content:space-between;margin-bottom:4px"><span>Swap</span><span>'+formatSize(swUsed)+' / '+formatSize(swTotal)+'</span></div>';
            html+='<div class="progress-bar"><div class="fill '+sColor+'" style="width:'+swPctNum+'%"></div></div>';
          }
          memEl.innerHTML=html;
        }

        // Network update (delta from previous sample)
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
    if(e.message!=='Unauthorized')document.getElementById('ds-cpu-cores').textContent='Error: '+e.message;
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

async function renderFileList(){
  const c=document.getElementById('content');
  if(!c)return;
  c.innerHTML=`
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
    let html='<div class="table-wrap"><table class="file-list-table"><thead><tr><th>Name</th><th>Size</th><th>Modified</th></tr></thead><tbody>';
    if(d.path&&d.path!==path){
      const parent=d.path.replace(/\/?[^\/]*\/?$/,'')||'/';
      html+=`<tr><td class="clickable" data-path="${escapeHtml(parent)}">.. (parent)</td><td></td><td></td></tr>`;
    }
    (d.dirs||[]).forEach(dir=>{
      const fullPath=(path.endsWith('/')?path:path+'/')+dir.name;
      html+=`<tr><td class="clickable" data-path="${escapeHtml(fullPath)}" data-type="dir">📁 ${escapeHtml(dir.name)}</td><td></td><td>${dir.modified_time?formatTime(dir.modified_time):''}</td></tr>`;
    });
    (d.files||[]).forEach(f=>{
      const fullPath=(path.endsWith('/')?path:path+'/')+f.name;
      html+=`<tr><td class="clickable" data-path="${escapeHtml(fullPath)}" data-type="file">${escapeHtml(f.name)}</td><td>${f.size!=null?formatSize(f.size):''}</td><td>${f.modified_time?formatTime(f.modified_time):''}</td></tr>`;
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
