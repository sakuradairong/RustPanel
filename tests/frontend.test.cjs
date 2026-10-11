const {test}=require('node:test');
const assert=require('node:assert/strict');
const vm=require('node:vm');
const fs=require('node:fs');

function harness(){
  const elements=new Map();
  function element(id){
    if(!elements.has(id))elements.set(id,{innerHTML:'',value:'',disabled:false,
      classList:{add(){},remove(){}},querySelectorAll(){return[]},
      addEventListener(name,handler){this[name]=handler}});
    return elements.get(id);
  }
  const storage=new Map([['token','test-token']]);
  const requests=[],routes=[];
  const context=vm.createContext({
    document:{getElementById:element},localStorage:{getItem:k=>storage.get(k)||null,setItem:(k,v)=>storage.set(k,v)},
    getToken:()=>storage.get('token'),clearToken:()=>{storage.delete('token');storage.delete('user')},
    navigate:route=>routes.push(route),cleanupPage(){},escapeHtml:s=>String(s),
    showToast(){},api:async(path,options)=>{requests.push({path,options});return{success:true}},
  });
  vm.runInContext(fs.readFileSync('public/app.js','utf8'),context);
  return{context,element,storage,requests,routes,run:code=>vm.runInContext(code,context)};
}

test('refresh and direct routes enforce password change before rendering a privileged page',async()=>{
  const h=harness();let layouts=0;
  h.context.api=async()=>({id:1,must_change_password:true,permissions:['docker:read']});
  h.context.layout=()=>layouts++;
  await h.run("render('/docker/containers')");
  assert.match(h.element('app').innerHTML,/change-password-form/);
  assert.equal(layouts,0);
});

test('logout calls revocation first and keeps local credentials on server failure',async()=>{
  const h=harness();
  h.context.api=async()=>({success:false,message:'unavailable'});
  await assert.rejects(h.run('signOut()'),/unavailable/);
  assert.equal(h.storage.get('token'),'test-token');
  assert.deepEqual(h.routes,[]);
  h.context.api=async(path,options)=>{
    assert.equal(path,'/login/logout');assert.equal(options.method,'POST');
    assert.equal(h.storage.get('token'),'test-token');return{success:true};
  };
  await h.run('signOut()');
  assert.equal(h.storage.has('token'),false);
  assert.deepEqual(h.routes,['/login']);
});

test('password form validates confirmation and clears revoked credentials after success',async()=>{
  const h=harness();h.run('renderChangePassword()');
  h.element('change-current').value='old-password';
  h.element('change-new').value='new-password';
  h.element('change-confirm').value='different';
  await h.element('change-password-form').onsubmit({preventDefault(){}});
  assert.equal(h.requests.length,0);
  assert.match(h.element('change-password-error').textContent,/do not match/);
  h.element('change-confirm').value='new-password';
  await h.element('change-password-form').onsubmit({preventDefault(){}});
  assert.equal(h.requests[0].path,'/user/change-password');
  assert.deepEqual(JSON.parse(h.requests[0].options.body),{current_password:'old-password',new_password:'new-password'});
  assert.equal(h.storage.has('token'),false);
});

test('file page starts at a server-provided custom root and excludes ancestor breadcrumbs',async()=>{
  const h=harness();
  h.context.api=async(path)=>{
    h.requests.push({path});
    if(path==='/file/roots')return{success:true,data:{available:['/srv/custom']}};
    return{success:true,data:{path:'/srv/custom',files:[],dirs:[]}};
  };
  await h.run('renderFileList()');
  assert.match(h.requests[1].path,/path=%2Fsrv%2Fcustom/);
  assert.doesNotMatch(h.element('file-breadcrumb').innerHTML,/data-path="\/"/);
  assert.doesNotMatch(h.element('file-breadcrumb').innerHTML,/data-path="\/srv"/);
  assert.match(h.element('file-breadcrumb').innerHTML,/data-path="\/srv\/custom"/);
});

test('viewer write controls are hidden while operator controls remain available',()=>{
  const h=harness();
  const button={dataset:{permission:'docker:write'},hidden:false};
  h.context.controlRoot={querySelectorAll:()=>[button]};
  h.run("currentUser={permissions:['docker:read']};applyPermissions(controlRoot)");
  assert.equal(button.hidden,true);
  h.run("currentUser={permissions:['docker:read','docker:write']};applyPermissions(controlRoot)");
  assert.equal(button.hidden,false);
});
