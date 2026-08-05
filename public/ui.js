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
