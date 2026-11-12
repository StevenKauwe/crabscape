import {createPeer,networkConfiguration,networkNotice,publicOffer,validateNetworkSdp} from '/network.js';
import {checkDeployment} from '/deployment.js';
import {validateMatchbox,matchboxAnswer} from '/matchbox.js';
import init,{BrowserChannel} from '/remote/pkg/harbor_noise.js';
const $=id=>document.getElementById(id);
let deploymentLocked=false,deploymentReady=false;
function lockDeployment(){deploymentLocked=true;if(deploymentReady)reset('Locked: service worker control changed.');$('deployment-controls').disabled=true;document.querySelectorAll('input,button,select,textarea').forEach(e=>e.disabled=true);$('status').textContent='Locked: deployment policy changed. Reopen on an approved clean origin.';}
navigator.serviceWorker?.addEventListener('controllerchange',lockDeployment);
try{await checkDeployment();if(deploymentLocked)throw new Error('Deployment changed');}catch{deploymentLocked=true;$('status').textContent='Locked: this deployment lacks the required trusted hosting policy. Do not load invitations or enter credentials here.';throw new Error('CrabBridge deployment policy refused');}
let signalAbort=null;
let passkeys=false,passkeyBusy=false,credentialAbort=null;
let peerNetworkContext={},offerDescription=null;
let invitation=null,pc=null,dc=null,noise=null,pending=null,epoch=0;
let authenticated=false,active=false,finished=false,stopping=false,sequence=0,offset=0,input='',control=false;
let terminal;
function freshTerminal(){if(terminal)terminal.dispose();$('terminal').replaceChildren();terminal=new window.Terminal({cols:100,rows:30,scrollback:1000,allowProposedApi:false,linkHandler:{activate(){}},theme:{background:'#15191f',foreground:'#e6e9ef'}});terminal.open($('terminal'));terminal.onData(text=>{if(!authenticated||!active||finished||stopping)return;if(input.length+text.length>8192){reset('Input queue overflow. Session closed to avoid losing or replaying commands.');return;}input+=text;});}
freshTerminal();
function status(text){$('status').textContent=text;}
function controls(){$('passkey-login').disabled=deploymentLocked||!passkeys||authenticated||passkeyBusy||Boolean(pending);const deviceReady=passkeys&&authenticated&&!active&&!passkeyBusy&&!pending&&!deploymentLocked;$('devices-section').hidden=!passkeys;$('passkey-register').disabled=!deviceReady;$('devices-refresh').disabled=!deviceReady;$('device-list').disabled=!deviceReady;$('device-remove').disabled=!deviceReady||!$('device-list').value;$('offer').disabled=!invitation;const ready=Boolean(noise?.ready()&&dc?.readyState==='open');$('login').disabled=!ready||authenticated||passkeyBusy||Boolean(pending);$('logout').disabled=!authenticated||Boolean(pending);$('tool').disabled=!authenticated||active;$('start').disabled=!authenticated||!control||active||passkeyBusy||Boolean(pending);$('stop').disabled=!active||Boolean(pending);}
function reset(reason){peerNetworkContext={};offerDescription=null;epoch++;credentialAbort?.abort();credentialAbort=null;passkeyBusy=false;passkeys=false;$('device-list').replaceChildren();if(signalAbort){signalAbort.abort();signalAbort=null;}if(pending){clearTimeout(pending.timer);pending.reject(new Error(reason));pending=null;}authenticated=false;active=false;finished=false;stopping=false;control=false;input='';sequence=0;offset=0;freshTerminal();invitation=null;$('invite').value='';$('password').value='';$('tool').replaceChildren();if(dc){dc.onclose=null;dc.close();dc=null;}if(pc){pc.close();pc=null;}if(noise){noise.free();noise=null;}$('connect').disabled=true;$('download-offer').disabled=true;$('offer-json').value='';$('answer-json').value='';$('identity').textContent='No invitation loaded.';controls();status(reason);}
function rpc(message){if(pending)throw new Error('A request is already in flight.');if(!noise?.ready()||dc?.readyState!=='open')throw new Error('Pair first.');const generation=epoch;return new Promise((resolve,reject)=>{const timer=setTimeout(()=>{if(epoch===generation)reset('Request timed out. Session closed; no automatic retry.');},15000);pending={resolve,reject,timer,generation};try{dc.send(noise.encrypt(new TextEncoder().encode(JSON.stringify(message))));controls();}catch(error){clearTimeout(timer);pending=null;reject(error);reset('Could not send; session closed.');}});}
async function terminalRpc(request){const generation=epoch;const reply=await rpc({method:'terminal',request:{...request,sequence}});if(generation!==epoch)throw new Error('Discarded old-session reply');if(reply.status!==200){reset(`Tool request refused (${reply.status}); no automatic retry.`);throw new Error('Session closed.');}const body=reply.body;sequence=body.next_sequence;if(body.error){active=false;input='';throw new Error(body.error);}return body.result;}
async function guarded(fn){const generation=epoch;try{await fn();}catch{if(epoch===generation)reset('Operation failed. Connection and queued input cleared; no automatic retry.');}finally{controls();}}
function validateAnswer(value){if(!value||Object.keys(value).sort().join(',')!=='sdp,type'||value.type!=='answer'||typeof value.sdp!=='string'||new TextEncoder().encode(value.sdp).length>65536||!value.sdp.startsWith('v=0\r\n')||value.sdp.includes('\0'))throw new Error('Invalid answer');validateNetworkSdp(peerNetworkContext,value.sdp);}
const SIGNAL_ENDPOINT='http://127.0.0.1:7879';
function validateSignal(signal){if(!signal||signal.endpoint!==SIGNAL_ENDPOINT||typeof signal.client_token!=='string'||!(/^[0-9a-fA-F]{64}$/).test(signal.client_token))throw new Error('Unsupported signaling destination');}
async function signalRequest(signal,path,method,body,abort){
 const response=await fetch(SIGNAL_ENDPOINT+path,{method,headers:{Authorization:`Bearer ${signal.client_token}`,...(body?{'Content-Type':'application/json'}:{})},body:body?JSON.stringify(body):undefined,credentials:'omit',redirect:'error',cache:'no-store',referrerPolicy:'no-referrer',signal:AbortSignal.any([abort,AbortSignal.timeout(5000)])});
 if(response.status===204||response.status===201)return{status:response.status};
 if(response.status!==200)throw new Error('Signaling refused');
 const reader=response.body.getReader();let length=0,chunks=[];
 while(true){const {done,value}=await reader.read();if(done)break;length+=value.length;if(length>65536*6+128){await reader.cancel();throw new Error('Signaling response too large');}chunks.push(value);}
 const bytes=new Uint8Array(length);let at=0;for(const chunk of chunks){bytes.set(chunk,at);at+=chunk.length;}
 return{status:200,body:JSON.parse(new TextDecoder().decode(bytes))};
}
async function autoSignal(signal,expires,generation){
 validateSignal(signal);signalAbort=new AbortController();const abort=signalAbort.signal;
 status('Exchanging connection details with local rendezvous. No password or terminal data goes there.');
 const sent=await signalRequest(signal,'/offer','POST',offerDescription,abort);
 if(sent.status!==201)throw new Error('Offer not accepted');
 while(epoch===generation&&Date.now()<expires*1000){
  const reply=await signalRequest(signal,'/answer','GET',null,abort);
  if(epoch!==generation)return;
  if(reply.status===200){validateAnswer(reply.body);await pc.setRemoteDescription(reply.body);return;}
  await new Promise(resolve=>setTimeout(resolve,500));
 }
 throw new Error('Pairing expired or replaced');
}
// Preserve the selected File before reset clears its input.
$('invite').onchange=()=>{const file=$('invite').files?.[0];reset('Loading invitation locally…');guarded(async()=>{const generation=epoch;if(!file)throw new Error('Choose an invitation.');if(file.size>4096)throw new Error('Invitation too large');const text=await file.text();if(epoch!==generation)return;const value=JSON.parse(text);if(Object.keys(value).some(k=>!['key','context','signal','matchbox'].includes(k)))throw new Error('Unsupported invitation field');networkConfiguration(value.context);if(typeof value.key!=='string'||!value.context)throw new Error('Invalid invitation');if(value.signal)validateSignal(value.signal);if(value.matchbox)validateMatchbox(value.matchbox);if(value.context.network_profile==='direct-stun-v1'&&(value.signal||value.matchbox?.endpoint!=='wss://crabscape-introducer.fly.dev'))throw new Error('Direct mode requires the existing production rendezvous');if(value.signal&&value.matchbox)throw new Error('Ambiguous rendezvous');invitation=value;$('identity').textContent=`Assigned user: ${value.context.principal}. Expires: ${new Date(value.context.expires_at*1000).toLocaleString()}.`;status('Invitation loaded locally. '+networkNotice(value.context)+' Create an offer to continue.');});};
$('offer').onclick=()=>guarded(async()=>{if(!invitation)throw new Error('Load a fresh invitation first.');peerNetworkContext=invitation.context;const context=JSON.stringify(invitation.context),signal=invitation.signal,matchbox=invitation.matchbox,expires=invitation.context.expires_at;noise=new BrowserChannel(invitation.key,context);invitation=null;controls();const generation=epoch;pc=createPeer(peerNetworkContext);dc=pc.createDataChannel('harbor-v1',{ordered:true});dc.binaryType='arraybuffer';dc.onopen=()=>{if(epoch!==generation)return;try{dc.send(noise.begin());status('Authenticating encrypted peer channel…');}catch{reset('Peer handshake failed.');}};dc.onclose=()=>{if(epoch===generation)reset('Peer disconnected. Local input queue cleared.');};dc.onmessage=event=>{if(epoch!==generation)return;try{const bytes=new Uint8Array(event.data);if(!noise.ready()){noise.finish(bytes);status('Encrypted peer channel established. Checking host capabilities…');rpc({method:'capabilities'}).then(reply=>{if(epoch!==generation)return;passkeys=reply.passkeys===true&&Boolean(window.PublicKeyCredential);status('Encrypted peer channel established. Authenticate your assigned user.');controls();}).catch(()=>{if(epoch===generation)reset('Host capabilities could not be verified.');});controls();return;}const reply=JSON.parse(new TextDecoder().decode(noise.decrypt(bytes)));if(!pending)throw new Error('Unexpected response');const task=pending;pending=null;clearTimeout(task.timer);task.resolve(reply);controls();}catch{reset('Invalid peer response; connection closed.');}};await pc.setLocalDescription(await pc.createOffer());await new Promise((resolve,reject)=>{if(pc.iceGatheringState==='complete')return resolve();const timer=setTimeout(()=>reject(new Error('ICE gathering timed out')),10000);pc.onicegatheringstatechange=()=>{if(epoch!==generation){clearTimeout(timer);reject(new Error('Connection replaced'));}else if(pc.iceGatheringState==='complete'){clearTimeout(timer);resolve();}};});if(epoch!==generation)return;offerDescription=publicOffer(peerNetworkContext,pc.localDescription);if(matchbox){signalAbort=new AbortController();status('Exchanging connection details through CrabScape rendezvous. Secrets stay in the encrypted channel.');const answer=await matchboxAnswer(matchbox,offerDescription,expires,signalAbort.signal);if(epoch!==generation)return;validateAnswer(answer);await pc.setRemoteDescription(answer);return;}if(signal){await autoSignal(signal,expires,generation);return;}$('offer-json').value=JSON.stringify(offerDescription);$('download-offer').disabled=false;$('connect').disabled=false;status('Offer ready. Give it to the host remote-answer command, then apply its answer.');});
$('download-offer').onclick=()=>{const url=URL.createObjectURL(new Blob([$('offer-json').value],{type:'application/json'}));const a=document.createElement('a');a.href=url;a.download='harbor-offer.json';a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);};
$('answer-file').onchange=()=>guarded(async()=>{const generation=epoch;const file=$('answer-file').files?.[0];if(!file||file.size>65536)throw new Error('Invalid answer file');const text=await file.text();if(epoch!==generation)return;$('answer-json').value=text;$('answer-file').value='';});
$('connect').onclick=()=>guarded(async()=>{if(!pc)throw new Error('Create an offer first');const data=$('answer-json').value;if(new TextEncoder().encode(data).length>65536*6+128)throw new Error('Answer too large');const answer=JSON.parse(data);validateAnswer(answer);await pc.setRemoteDescription(answer);$('connect').disabled=true;});
$('disconnect').onclick=()=>reset('Disconnected. Host connection cleanup follows channel closure.');

$('login').onclick=()=>guarded(async()=>{input='';const generation=epoch;const password=$('password').value;$('password').value='';const reply=await rpc({method:'login',password});if(generation!==epoch)throw new Error('Discarded old login response');applyLogin(reply);});
function applyLogin(reply){
 if(!Array.isArray(reply.tools)||typeof reply.control!=='boolean'||!Number.isSafeInteger(reply.next_sequence))throw new Error('Invalid login response');
 authenticated=true;control=reply.control;sequence=reply.next_sequence;
 $('tool').replaceChildren(...reply.tools.map(name=>{if(typeof name!=='string')throw new Error('Invalid tool');const option=document.createElement('option');option.value=name;option.textContent=name;return option;}));
 status('Authenticated. Only the listed host-registered tools are available.');controls();
}
function binary(value){
 if(typeof value!=='string'||!(/^[A-Za-z0-9_-]+$/).test(value))throw new Error('Invalid WebAuthn binary value');
 return Uint8Array.from(atob(value.replace(/-/g,'+').replace(/_/g,'/')),c=>c.charCodeAt(0));
}
function encoded(value){return btoa(String.fromCharCode(...new Uint8Array(value))).replace(/\+/g,'-').replace(/\//g,'_').replace(/=+$/,'');}
function optionsJSON(value,registration){
 const options=structuredClone(value?.publicKey||value);if(!options||typeof options!=='object')throw new Error('Missing WebAuthn options');
 options.challenge=binary(options.challenge);
 if(registration){if(!options.user)throw new Error('Missing WebAuthn user');options.user.id=binary(options.user.id);}
 for(const name of ['excludeCredentials','allowCredentials'])if(options[name])options[name]=options[name].map(item=>({...item,id:binary(item.id)}));
 return options;
}
function credentialJSON(credential){
 if(!credential||credential.type!=='public-key')throw new Error('Missing public-key credential');
 const response={};for(const field of ['clientDataJSON','attestationObject','authenticatorData','signature','userHandle']){
  const value=credential.response[field];if(value!==undefined)response[field]=value===null?null:encoded(value);
 }
 if(credential.response.getTransports)response.transports=credential.response.getTransports();
 return{id:credential.id,rawId:encoded(credential.rawId),type:credential.type,response,clientExtensionResults:credential.getClientExtensionResults(),authenticatorAttachment:credential.authenticatorAttachment};
}
async function passkeyOperation(registration){
 if(!passkeys||passkeyBusy||pending||active||deploymentLocked||(registration&&!authenticated)||(!registration&&authenticated))return;
 const generation=epoch;passkeyBusy=true;credentialAbort=new AbortController();controls();
 try{
  const password=registration?$('password').value:undefined;if(registration)$('password').value='';
  const begin=await rpc(registration?{method:'passkey_register_begin',password}:{method:'passkey_login_begin'});
  if(generation!==epoch)return;
  if(!begin.options)throw new Error('Host did not provide passkey options');
  const publicKey=optionsJSON(begin.options,registration);
  const credential=await navigator.credentials[registration?'create':'get']({publicKey,signal:credentialAbort.signal});
  if(generation!==epoch||deploymentLocked)return;
  const reply=await rpc({method:registration?'passkey_register_finish':'passkey_login_finish',credential:credentialJSON(credential)});
  if(generation!==epoch)return;
  if(registration){if(reply.registered!==true)throw new Error('Device enrollment rejected');status('Passkey registered. Private invitations are still required.');}
  else applyLogin(reply);
 }catch(error){if(generation===epoch){if(error.name==='NotAllowedError'||error.name==='AbortError')reset('Passkey operation cancelled. Connection closed to discard the pending ceremony.');else reset('Passkey operation failed. Connection closed; no automatic retry.');}}
 finally{if(generation===epoch){credentialAbort=null;passkeyBusy=false;controls();}}
}
$('passkey-login').onclick=()=>passkeyOperation(false);
$('passkey-register').onclick=()=>passkeyOperation(true);
$('device-list').onchange=controls;
async function refreshDevices(){
 const generation=epoch,devices=[];let offset=0;
 for(let page=0;page<2;page++){
  const reply=await rpc({method:'device_list',offset});if(generation!==epoch)return;
  if(!Array.isArray(reply.devices)||reply.devices.length>4)throw new Error('Invalid passkey list');devices.push(...reply.devices);
  if(reply.next===null||reply.next===undefined){offset=null;break;}
  if(!Number.isSafeInteger(reply.next)||reply.next<=offset)throw new Error('Invalid passkey cursor');offset=reply.next;
 }
 if(offset!==null)throw new Error('Passkey listing exceeds supported bound');
 $('device-list').replaceChildren(...devices.filter(device=>device.revoked!==true).map(device=>{if(typeof device.id!=='string')throw new Error('Invalid passkey identity');const option=document.createElement('option');option.value=device.id;option.textContent=`${device.id} · expires ${device.expires}`;return option;}));controls();
}
$('devices-refresh').onclick=()=>guarded(async()=>{if(!authenticated||!passkeys||active||passkeyBusy)return;await refreshDevices();});
$('device-remove').onclick=()=>guarded(async()=>{if(!authenticated||!passkeys||active||passkeyBusy||!$('device-list').value)return;const generation=epoch,reply=await rpc({method:'device_remove',id:$('device-list').value});if(generation!==epoch)return;if(reply.error)throw new Error('Removal rejected');await refreshDevices();status('Selected passkey removed.');});
$('logout').onclick=()=>guarded(async()=>{input='';stopping=true;const generation=epoch;try{await rpc({method:'logout'});}finally{if(generation===epoch)reset('Logged out. Connection closed.');}});
$('start').onclick=()=>guarded(async()=>{input='';stopping=false;finished=false;offset=0;terminal.reset();await terminalRpc({action:'start',tool:$('tool').value});active=true;terminal.focus();status('Registered tool running. Its existing permissions apply.');});
$('stop').onclick=()=>guarded(async()=>{input='';stopping=true;try{await terminalRpc({action:'stop'});active=false;status('Stop requested. Input queue cleared.');}finally{stopping=false;}});
setInterval(()=>{if(!authenticated||!active||finished||stopping||pending||passkeyBusy)return;const generation=epoch;guarded(async()=>{if(input){const text=input.slice(0,1024);input=input.slice(1024);await terminalRpc({action:'input',text});}else{const result=await terminalRpc({action:'read',offset});if(generation!==epoch)return;offset=result.offset;if(result.truncated)status('Earlier output was truncated by host retention limit.');if(Array.isArray(result.bytes))terminal.write(new Uint8Array(result.bytes));if(result.ended){finished=true;input='';status('Tool exited. Press Stop tool to release the session before starting another.');}}}).catch(()=>{});},200);
await init();if(deploymentLocked)throw new Error('Deployment locked');deploymentReady=true;$('deployment-controls').disabled=false;status('Ready. Load a private host-generated invitation file.');controls();
