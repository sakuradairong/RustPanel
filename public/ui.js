// ── Shared UI primitives ──
// Reusable modal / confirm-dialog helpers so pages stop hand-rolling the same
// `.modal-overlay` + `.modal` + `.btn-row` boilerplate. Also adds behaviour that
// every consumer gets for free: ESC-to-close, click-outside-to-close, initial
// focus, and focus restoration to the previously focused element.

// Build initials (max 2 chars) from a display name; used for the topbar avatar.
function initials(name){
  const n=(name||'').trim();
  if(!n)return '?';
  const parts=n.split(/[\s._-]+/).filter(Boolean);
  if(parts.length>=2)return (parts[0][0]+parts[1][0]);
  return n.slice(0,2);
}

// Open a modal. `innerHTML` is placed inside `.modal`. Returns { overlay, root, close }.
// opts: { maxWidth, closeOnOverlay=true, onClose }
function openModal(innerHTML,opts){
  opts=opts||{};
  const prevFocus=document.activeElement;
  const overlay=document.createElement('div');
  overlay.className='modal-overlay';
  const style=opts.maxWidth?` style="max-width:${opts.maxWidth}"`:'';
  overlay.innerHTML=`<div class="modal" role="dialog" aria-modal="true"${style}>${innerHTML}</div>`;
  const root=overlay.querySelector('.modal');
  let closed=false;
  function close(){
    if(closed)return;
    closed=true;
    document.removeEventListener('keydown',onKey);
    overlay.remove();
    if(opts.onClose){try{opts.onClose()}catch(e){}}
    if(prevFocus&&prevFocus.focus){try{prevFocus.focus()}catch(e){}}
  }
  function onKey(e){if(e.key==='Escape')close()}
  if(opts.closeOnOverlay!==false)overlay.addEventListener('click',e=>{if(e.target===overlay)close()});
  document.addEventListener('keydown',onKey);
  document.body.appendChild(overlay);
  const focusEl=root.querySelector('input,select,textarea,button');
  if(focusEl)setTimeout(()=>{try{focusEl.focus()}catch(e){}},0);
  return {overlay,root,close};
}

// Confirmation dialog with an async action button.
// opts: { title, message | messageHtml, okText, cancelText, danger, okClass,
//         loadingText, maxWidth, onConfirm }
// onConfirm contract:
//   - resolve normally  -> dialog closes
//   - resolve `true`     -> dialog stays open (e.g. validation failed)
//   - throw an Error     -> message shown inline, dialog stays open
// Lightweight, dependency-free rolling SVG line chart for live metrics.
// svg: an <svg> element. opts: { capacity, max (fixed) or null for auto,
//   series: [{ key, color }] }. Returns { push(valuesByKey) }.
function createLiveChart(svg,opts){
  opts=opts||{};
  const cap=opts.capacity||60;
  const series=opts.series||[{key:'v',color:'var(--primary)'}];
  const fixedMax=opts.max||null;
  const NS='http://www.w3.org/2000/svg';
  const buffers={};
  series.forEach(s=>{buffers[s.key]=[]});
  svg.setAttribute('viewBox','0 0 100 100');
  svg.setAttribute('preserveAspectRatio','none');
  svg.innerHTML='';
  [25,50,75].forEach(y=>{
    const l=document.createElementNS(NS,'line');
    l.setAttribute('x1',0);l.setAttribute('x2',100);l.setAttribute('y1',y);l.setAttribute('y2',y);
    l.setAttribute('class','lc-grid');l.setAttribute('vector-effect','non-scaling-stroke');
    svg.appendChild(l);
  });
  const parts={};
  series.forEach(s=>{
    const area=document.createElementNS(NS,'path');
    area.setAttribute('fill',s.color);area.setAttribute('opacity','0.10');area.setAttribute('stroke','none');
    svg.appendChild(area);
    const line=document.createElementNS(NS,'polyline');
    line.setAttribute('fill','none');line.setAttribute('stroke',s.color);line.setAttribute('stroke-width','1.6');
    line.setAttribute('vector-effect','non-scaling-stroke');line.setAttribute('stroke-linejoin','round');line.setAttribute('stroke-linecap','round');
    svg.appendChild(line);
    parts[s.key]={line,area};
  });
  const step=cap>1?100/(cap-1):0;
  function redraw(){
    let max=fixedMax;
    if(!max){max=1;series.forEach(s=>buffers[s.key].forEach(v=>{if(v>max)max=v}));max*=1.25}
    series.forEach(s=>{
      const buf=buffers[s.key];
      const n=buf.length;
      const pts=[];
      for(let i=0;i<n;i++){
        const x=100-(n-1-i)*step;
        const y=100-Math.max(0,Math.min(100,(buf[i]/max)*100));
        pts.push(x.toFixed(2)+','+y.toFixed(2));
      }
      parts[s.key].line.setAttribute('points',pts.join(' '));
      if(n>0){
        const x0=(100-(n-1)*step).toFixed(2);
        parts[s.key].area.setAttribute('d','M'+x0+',100 L'+pts.join(' L')+' L100,100 Z');
      } else {
        parts[s.key].area.removeAttribute('d');
      }
    });
  }
  return {
    push(vals){
      series.forEach(s=>{const b=buffers[s.key];b.push(Number(vals[s.key])||0);if(b.length>cap)b.shift()});
      redraw();
    }
  };
}

function confirmDialog(opts){
  opts=opts||{};
  const okText=opts.okText||'Confirm';
  const cancelText=opts.cancelText||'Cancel';
  const okClass=opts.danger?'btn-danger':(opts.okClass||'btn-success');
  const body=opts.messageHtml?opts.messageHtml:escapeHtml(opts.message||'Are you sure?');
  const html=`
    <h3>${escapeHtml(opts.title||'Confirm')}</h3>
    <p style="color:var(--text-muted);font-size:.9rem;margin-bottom:1rem">${body}</p>
    <div class="error-msg" data-role="error" style="display:none"></div>
    <div class="btn-row">
      <button class="btn btn-sm btn-ghost" data-role="cancel">${escapeHtml(cancelText)}</button>
      <button class="btn btn-sm ${okClass}" data-role="ok">${escapeHtml(okText)}</button>
    </div>`;
  const m=openModal(html,{maxWidth:opts.maxWidth||'400px'});
  const okBtn=m.root.querySelector('[data-role=ok]');
  const cancelBtn=m.root.querySelector('[data-role=cancel]');
  const errEl=m.root.querySelector('[data-role=error]');
  cancelBtn.addEventListener('click',m.close);
  okBtn.addEventListener('click',async()=>{
    errEl.style.display='none';
    if(!opts.onConfirm){m.close();return}
    okBtn.disabled=true;
    const orig=okBtn.textContent;
    okBtn.textContent=opts.loadingText||'Working...';
    try{
      const keepOpen=await opts.onConfirm();
      if(keepOpen===true){okBtn.disabled=false;okBtn.textContent=orig;return}
      m.close();
    }catch(e){
      if(e&&e.message==='Unauthorized'){m.close();return}
      errEl.textContent=(e&&e.message)?e.message:String(e);
      errEl.style.display='block';
      okBtn.disabled=false;okBtn.textContent=orig;
    }
  });
  return m;
}
