'use strict';
const $ = id => document.getElementById(id);
let token=null, sequence=0, offset=0, running=false, owned=false, busy=false, queued='', generation=0;
let term;
function freshTerminal(){
 if(term)term.dispose();
 const gen=generation;
 term=new Terminal({cols:100,rows:30,scrollback:2000,linkHandler:{activate:()=>{}},theme:{background:'#15191f'}});
 term.open($('terminal'));
 term.onData(text=>{if(gen===generation&&running&&token){if(queued.length+text.length>16384){$('result').textContent='Input buffer full; wait for the tool.';return;}queued+=text;}});
}
freshTerminal();
function controls(){
 $('login').disabled=busy||Boolean(token);
 $('start').disabled=busy||!token||owned;
 $('stop').disabled=busy||!owned;
 $('send').disabled=busy||!running;
 $('interrupt').disabled=busy||!running;
 $('logout').disabled=!token; // Revocation remains usable during an in-flight request.
}
function clearInput(){queued='';$('input').value='';}
function reset(){generation++;running=false;clearInput();offset=0;}
async function request(path,data,credential=token){
 const response=await fetch(path,{method:'POST',headers:{'Content-Type':'application/json',...(credential?{Authorization:`Bearer ${credential}`}:{})},body:data===undefined?undefined:JSON.stringify(data),credentials:'omit',cache:'no-store',signal:AbortSignal.timeout(15000)});
 if(!response.ok)throw new Error(`Request refused (${response.status}). Reconnect after checking the host; do not blindly retry tool commands.`);
 return response.status===204?{}:response.json();
}
$('login').onclick=async()=>{
 if(busy||token)return;
 reset();freshTerminal();const gen=generation;busy=true;controls();const password=$('password').value;$('password').value='';
 try{
  const data=await request('/api/login',{username:$('username').value,password},null);
  if(gen!==generation){await request('/api/logout',undefined,data.token);return;}
  token=data.token;sequence=data.next_sequence;
  $('tool').replaceChildren(...data.tools.map(name=>{const o=document.createElement('option');o.value=name;o.textContent=name;return o;}));
  $('result').textContent='Authenticated for 15 minutes. Tool control includes the tool’s permission prompts.';
 }catch(e){$('result').textContent=e.message;}finally{busy=false;controls();}
};
$('logout').onclick=async()=>{
 const old=token;token=null;owned=false;reset();freshTerminal();sequence=0;controls();
 try{if(old)await request('/api/logout',undefined,old);$('result').textContent='Session revoked.';}
 catch(e){$('result').textContent='Local token forgotten, but host revocation is unconfirmed. Stop the host if needed. '+e.message;}
 finally{controls();}
};
async function terminal(action,extra={}){
 if(busy||!token)return;
 if(action==='start'||action==='stop'){reset();}if(action==='start')owned=true;
 const gen=generation;busy=true;controls();
 try{
  const data=await request('/api/terminal',{action,sequence,...extra});
  if(gen!==generation)return;
  sequence=data.next_sequence;
  if(data.error)throw new Error(data.error);
  if(action==='start'){running=true;offset=0;freshTerminal();term.focus();}
  if(data.result.bytes!==undefined){
   offset=data.result.offset;
   if(data.result.truncated){freshTerminal();$('result').textContent='Old output truncated by host memory limit.';}
   term.write(new Uint8Array(data.result.bytes));
   if(data.result.ended){running=false;owned=false;clearInput();$('result').textContent='Tool exited; host slot released.';}
  }
  if(action==='stop'){owned=false;$('result').textContent='Tool session stopped; immediate child cleanup requested.';}
 }catch(e){if(gen===generation){running=false;clearInput();$('result').textContent=e.message;}}
 finally{busy=false;controls();}
}
$('start').onclick=()=>terminal('start',{tool:$('tool').value});
$('stop').onclick=()=>terminal('stop');
$('interrupt').onclick=()=>terminal('input',{text:'\x03'});
$('send').onclick=()=>{if(busy||!running)return;const text=$('input').value+'\r';$('input').value='';terminal('input',{text});};
$('input').onkeydown=e=>{if(e.key==='Enter'){e.preventDefault();$('send').click();}};
setInterval(()=>{if(queued&&running&&!busy){const text=queued.slice(0,4096);queued=queued.slice(text.length);terminal('input',{text});}},30);
setInterval(()=>{if(running&&!busy)terminal('read',{offset});},500);
controls();