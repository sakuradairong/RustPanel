// ── MD5 ──
const MD5_S = [7,12,17,22,5,9,14,20,4,11,16,23,6,10,15,21];
const MD5_K = new Uint32Array(64);
for(let i=0;i<64;i++) MD5_K[i] = Math.abs(Math.sin(i+1)) * 0x100000000 | 0;

function md5_cycle(x,k) {
  let a=x[0],b=x[1],c=x[2],d=x[3];
  for(let i=0;i<64;i++){
    let f,g;
    if(i<16){f=(b&c)|(~b&d);g=i}
    else if(i<32){f=(d&b)|(~d&c);g=(5*i+1)&15}
    else if(i<48){f=b^c^d;g=(3*i+5)&15}
    else{f=c^(b|~d);g=(7*i)&15}
    f = (f + a + MD5_K[i] + k[g]) | 0;
    const s = MD5_S[i%4 + ((i>>4)<<2)];
    a = d; d = c; c = b;
    b = (b + ((f << s) | (f >>> (32 - s)))) | 0;
  }
  x[0] = (x[0] + a) | 0;
  x[1] = (x[1] + b) | 0;
  x[2] = (x[2] + c) | 0;
  x[3] = (x[3] + d) | 0;
}
function md5(str){
  const bytes = [];
  for(let i=0;i<str.length;i++){
    let c = str.charCodeAt(i);
    if(c<128) bytes.push(c);
    else if(c<2048){bytes.push(192|c>>6);bytes.push(128|(c&63))}
    else{bytes.push(224|c>>12);bytes.push(128|(c>>6&63));bytes.push(128|(c&63))}
  }
  const orig_len = bytes.length;
  bytes.push(0x80);
  while(bytes.length%64!==56) bytes.push(0);
  const bit_len = orig_len * 8;
  bytes.push(bit_len & 0xff);
  bytes.push((bit_len >> 8) & 0xff);
  bytes.push((bit_len >> 16) & 0xff);
  bytes.push((bit_len >> 24) & 0xff);
  bytes.push(0); bytes.push(0); bytes.push(0); bytes.push(0);
  const h = [0x67452301,0xefcdab89,0x98badcfe,0x10325476];
  const w = new Int32Array(16);
  for(let i=0;i<bytes.length;i+=64){
    for(let j=0;j<16;j++) w[j]=bytes[i+4*j]|(bytes[i+4*j+1]<<8)|(bytes[i+4*j+2]<<16)|(bytes[i+4*j+3]<<24);
    md5_cycle(h,w);
  }
  let hex='';
  for(let i=0;i<4;i++) for(let j=0;j<4;j++) hex+=((h[i]>>>(j*8))&0xff).toString(16).padStart(2,'0');
  return hex;
}

// ── SM4 ──
const S8=[0xd6,0x90,0xe9,0xfe,0xcc,0xe1,0x3d,0xb7,0x16,0xb6,0x14,0xc2,0x28,0xfb,0x2c,0x05,0x2b,0x67,0x9a,0x76,0x2a,0xbe,0x04,0xc3,0xaa,0x44,0x13,0x26,0x49,0x86,0x06,0x99,0x9c,0x42,0x50,0xf4,0x91,0xef,0x98,0x7a,0x33,0x54,0x0b,0x43,0xed,0xcf,0xac,0x62,0xe4,0xb3,0x1c,0xa9,0xc9,0x08,0xe8,0x95,0x80,0xdf,0x94,0xfa,0x75,0x8f,0x3f,0xa6,0x47,0x07,0xa7,0xfc,0xf3,0x73,0x17,0xba,0x83,0x59,0x3c,0x19,0xe6,0x85,0x4f,0xa8,0x68,0x6b,0x81,0xb2,0x71,0x64,0xda,0x8b,0xf8,0xeb,0x0f,0x4b,0x70,0x56,0x9d,0x35,0x1e,0x24,0x0e,0x5e,0x63,0x58,0xd1,0xa2,0x25,0x22,0x7c,0x3b,0x01,0x21,0x78,0x87,0xd4,0x00,0x46,0x57,0x9f,0xd3,0x27,0x52,0x4c,0x36,0x02,0xe7,0xa0,0xc4,0xc8,0x9e,0xea,0xbf,0x8a,0xd2,0x40,0xc7,0x38,0xb5,0xa3,0xf7,0xf2,0xce,0xf9,0x61,0x15,0xa1,0xe0,0xae,0x5d,0xa4,0x9b,0x34,0x1a,0x55,0xad,0x93,0x32,0x30,0xf5,0x8c,0xb1,0xe3,0x1d,0xf6,0xe2,0x2e,0x82,0x66,0xca,0x60,0xc0,0x29,0x23,0xab,0x0d,0x53,0x4e,0x6f,0xd5,0xdb,0x37,0x45,0xde,0xfd,0x8e,0x2f,0x03,0xff,0x6a,0x72,0x6d,0x6c,0x5b,0x51,0x8d,0x1b,0xaf,0x92,0xbb,0xdd,0xbc,0x7f,0x11,0xd9,0x5c,0x41,0x1f,0x10,0x5a,0xd8,0x0a,0xc1,0x31,0x88,0xa5,0xcd,0x7b,0xbd,0x2d,0x74,0xd0,0x12,0xb8,0xe5,0xb4,0xb0,0x89,0x69,0x97,0x4a,0x0c,0x96,0x77,0x7e,0x65,0xb9,0xf1,0x09,0xc5,0x6e,0xc6,0x84,0x18,0xf0,0x7d,0xec,0x3a,0xdc,0x4d,0x20,0x79,0xee,0x5f,0x3e,0xd7,0xcb,0x39,0x48];
const FK=[0xa3b1bac6,0x56aa3350,0x677d9197,0xb27022dc];
const CK=[0x00070e15,0x1c232a31,0x383f464d,0x545b6269,0x70777e85,0x8c939aa1,0xa8afb6bd,0xc4cbd2d9,0xe0e7eef5,0xfc030a11,0x181f262d,0x343b4249,0x50575e65,0x6c737a81,0x888f969d,0xa4abb2b9,0xc0c7ced5,0xdce3eaf1,0xf8ff060d,0x141b2229,0x30373e45,0x4c535a61,0x686f767d,0x848b9299,0xa0a7aeb5,0xbcc3cad1,0xd8dfe6ed,0xf4fb0209,0x10171e25,0x2c333a41,0x484f565d,0x646b7279];

function rotl32(x,n){return((x<<n)|(x>>>(32-n)))>>>0}
function tau(x){
  return(S8[(x>>>24)&0xff]<<24)|(S8[(x>>>16)&0xff]<<16)|(S8[(x>>>8)&0xff]<<8)|S8[x&0xff]
}
function L(b){return b^rotl32(b,2)^rotl32(b,10)^rotl32(b,18)^rotl32(b,24)}
function Lp(b){return b^rotl32(b,13)^rotl32(b,23)}
function T(a){return L(tau(a))}
function Tp(a){return Lp(tau(a))}

function expandKey(keyHex){
  const mk=[];
  for(let i=0;i<4;i++)mk[i]=parseInt(keyHex.substr(i*8,8),16);
  const K=new Uint32Array(36);
  const rk=new Uint32Array(32);
  for(let i=0;i<4;i++)K[i]=(mk[i]^FK[i])>>>0;
  for(let i=0;i<32;i++){
    const t=(K[i+1]^K[i+2]^K[i+3]^CK[i])>>>0;
    K[i+4]=(K[i]^Tp(t))>>>0;
    rk[i]=K[i+4];
  }
  return rk;
}

function encryptBlock(X0,X1,X2,X3,rk){
  let x=[X0,X1,X2,X3];
  for(let i=0;i<32;i++){
    const t=(x[1]^x[2]^x[3]^rk[i])>>>0;
    x.push((x[0]^T(t))>>>0);
    x.shift();
  }
  return[x[3],x[2],x[1],x[0]];
}

function decryptBlock(X0,X1,X2,X3,rk){
  let x=[X0,X1,X2,X3];
  for(let i=0;i<32;i++){
    const t=(x[1]^x[2]^x[3]^rk[31-i])>>>0;
    x.push((x[0]^T(t))>>>0);
    x.shift();
  }
  return[x[3],x[2],x[1],x[0]];
}

function hex2bytes(h){
  const b=[];
  for(let i=0;i<h.length;i+=2)b.push(parseInt(h.substr(i,2),16));
  return b;
}
function bytes2hex(b){
  let h='';
  for(let i=0;i<b.length;i++)h+=((b[i]&0xff)>>>0).toString(16).padStart(2,'0');
  return h;
}
function bytes2words(b){
  const w=[];
  for(let i=0;i<b.length;i+=4)w.push(((b[i]<<24)|(b[i+1]<<16)|(b[i+2]<<8)|b[i+3])>>>0);
  return w;
}
function words2bytes(w){
  const b=[];
  for(let i=0;i<w.length;i++){b.push((w[i]>>>24)&0xff);b.push((w[i]>>>16)&0xff);b.push((w[i]>>>8)&0xff);b.push(w[i]&0xff)}
  return b;
}
function str2utf8(s){return Array.from(new TextEncoder().encode(s))}
function utf82str(b){return new TextDecoder().decode(new Uint8Array(b))}

function pkcs7pad(d){const n=16-(d.length%16);const r=[...d];for(let i=0;i<n;i++)r.push(n);return r}
function pkcs7unpad(d){
  const n=d[d.length-1];
  if(n<1||n>16)throw new Error('invalid pkcs7 padding');
  return d.slice(0,d.length-n);
}

function sm4_cbc_encrypt_hex(plain,keyHex,ivHex){
  const keyB=hex2bytes(keyHex),ivB=hex2bytes(ivHex);
  const pb=str2utf8(plain),padded=pkcs7pad(pb);
  const rk=expandKey(keyHex);
  let prev=bytes2words(ivB);
  const out=[];
  for(let i=0;i<padded.length;i+=16){
    let w=bytes2words(padded.slice(i,i+16));
    for(let j=0;j<4;j++)w[j]=(w[j]^prev[j])>>>0;
    const e=encryptBlock(w[0],w[1],w[2],w[3],rk);
    out.push(...e);prev=e;
  }
  return bytes2hex(words2bytes(out));
}

function sm4_cbc_decrypt_hex(cipherHex,keyHex,ivHex){
  const cb=hex2bytes(cipherHex);
  if(cb.length%16!==0)throw new Error('ciphertext length not multiple of 16');
  const rk=expandKey(keyHex);
  let prev=bytes2words(hex2bytes(ivHex));
  const out=[];
  for(let i=0;i<cb.length;i+=16){
    const w=bytes2words(cb.slice(i,i+16));
    const d=decryptBlock(w[0],w[1],w[2],w[3],rk);
    for(let j=0;j<4;j++)d[j]=(d[j]^prev[j])>>>0;
    out.push(...words2bytes(d));prev=w;
  }
  const unp=pkcs7unpad(out);
  return utf82str(unp);
}

// ── Config ──
const SECURITY_DIR = 'dfhg156d1';

// ── API ──
const API_BASE = '/api/v1';

function getToken(){return localStorage.getItem('token')}
function setToken(t){localStorage.setItem('token',t)}
function clearToken(){localStorage.removeItem('token');localStorage.removeItem('user')}

async function api(path,opts={}){
  const h=opts.headers||{};
  const t=getToken();
  if(t)h['Authorization']='Bearer '+t;
  if(opts.body&&!(opts.body instanceof FormData)&&!h['Content-Type'])h['Content-Type']='application/json';
  const r=await fetch(API_BASE+path,{...opts,headers:h});
  if(r.status===401){clearToken();navigate('/login');throw new Error('Unauthorized')}
  const j=await r.json();
  return j;
}

function apiUrl(path){return API_BASE+path}

// ── Router ──
function getHash(){return window.location.hash.replace(/^#\//,'/').split('?')[0]||'/home'}
function getHashParams(){
  const m=window.location.hash.match(/\?(.+)/);
  if(!m)return{};
  const p={};
  m[1].split('&').forEach(kv=>{const[k,v]=kv.split('=');if(k)p[decodeURIComponent(k)]=decodeURIComponent(v||'')});
  return p;
}

let currentRoute='';
function navigate(path,params){
  let h='#/'+path.replace(/^\//,'');
  if(params){
    const s=Object.entries(params).map(([k,v])=>encodeURIComponent(k)+'='+encodeURIComponent(v)).join('&');
    if(s)h+='?'+s;
  }
  window.location.hash=h;
}

function initRouter(){
  function handle(){
    const route=getHash();
    if(route===currentRoute&&route!=='/login')return;
    currentRoute=route;
    render(route);
  }
  window.addEventListener('hashchange',handle);
  handle();
}

// ── State cleanup ──
let monitorEventSource=null;
let processInterval=null;
let logRefreshInterval=null;
let logRefreshEnabled=false;
let currentLogFile='';
let currentLogKeyword='';
let processSearchKeyword='';

function cleanupPage(){
  if(monitorEventSource){monitorEventSource.close();monitorEventSource=null}
  if(dashboardEventSource){dashboardEventSource.close();dashboardEventSource=null}
  if(processInterval){clearInterval(processInterval);processInterval=null}
  if(logRefreshInterval){clearInterval(logRefreshInterval);logRefreshInterval=null;logRefreshEnabled=false}
}

// ── Toast ──
function showToast(message,type){
  if(!type)type='info';
  let container=document.querySelector('.toast-container');
  if(!container){
    container=document.createElement('div');
    container.className='toast-container';
    document.body.appendChild(container);
  }
  const toast=document.createElement('div');
  toast.className='toast '+type;
  toast.textContent=message;
  container.appendChild(toast);
  setTimeout(()=>{toast.style.opacity='0';toast.style.transition='opacity .3s';setTimeout(()=>toast.remove(),300)},3000);
}



function escapeHtml(s){const d=document.createElement("div");d.textContent=s;return d.innerHTML}
function formatTime(ts){if(!ts)return"";const d=new Date(ts*1000);return d.toLocaleString()}
