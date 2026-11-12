// CrabScape's existing Matchbox introducer is untrusted rendezvous, not authority.
export const MATCHBOX_ENDPOINT='wss://crabscape-introducer.fly.dev';
export const MATCHBOX_TEST_ENDPOINT='ws://127.0.0.1:3536';
const bytes=value=>new TextEncoder().encode(value).length;
export function validateMatchbox(signal){
 if(!signal||Object.keys(signal).sort().join(',')!=='endpoint,room'||![MATCHBOX_ENDPOINT,MATCHBOX_TEST_ENDPOINT].includes(signal.endpoint)||!/^crabscape-crabbridge-[0-9a-f]{32}$/.test(signal.room))throw new Error('Unsupported CrabScape rendezvous');
}
const peer=value=>typeof value==='string'&&/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value);
export function matchboxAnswer(signal,offer,expires,abort){
 validateMatchbox(signal);
 if(!offer||Object.keys(offer).sort().join(',')!=='sdp,type'||offer.type!=='offer'||typeof offer.sdp!=='string'||bytes(offer.sdp)>65536||!offer.sdp.startsWith('v=0\r\n')||offer.sdp.includes('\0'))throw new Error('Invalid offer');
 if(abort.aborted||Date.now()>=expires*1000||expires*1000>Date.now()+600000)throw new Error('Expired rendezvous');
 return new Promise((resolve,reject)=>{
  const ws=new WebSocket(`${signal.endpoint}/${signal.room}`);let own=null,host=null,sent=false,done=false,count=0;
  const finish=(error,answer)=>{if(done)return;done=true;clearTimeout(timer);clearInterval(keepalive);abort.removeEventListener('abort',cancel);ws.onmessage=null;ws.onclose=null;ws.onerror=null;ws.close();error?reject(error):resolve(answer);};
  const cancel=()=>finish(new Error('Pairing canceled'));
  const timer=setTimeout(()=>finish(new Error('Pairing expired')),Math.max(1,expires*1000-Date.now()));
  const keepalive=setInterval(()=>{if(ws.readyState===WebSocket.OPEN)ws.send(JSON.stringify('KeepAlive'));},15000);
  abort.addEventListener('abort',cancel,{once:true});
  ws.onerror=()=>finish(new Error('Rendezvous unavailable'));ws.onclose=()=>finish(new Error('Rendezvous disconnected'));
  ws.onmessage=event=>{try{
   if(abort.aborted||Date.now()>=expires*1000)throw new Error('Pairing ended');
   if(typeof event.data!=='string'||bytes(event.data)>65536*6+1024||++count>128)throw new Error('Rendezvous bounds');
   const value=JSON.parse(event.data);if(!value||Array.isArray(value)||Object.keys(value).length!==1)throw new Error('Invalid rendezvous event');
   if('IdAssigned'in value){if(own||!peer(value.IdAssigned))throw new Error('Invalid assignment');own=value.IdAssigned;return;}
   if(!own)throw new Error('Assignment required');
   if('PeerLeft'in value)throw new Error('Peer left');
   if('NewPeer'in value)throw new Error('Start the host before joining');
   const envelope=value.Signal;if(!envelope||Object.keys(envelope).sort().join(',')!=='data,sender'||!peer(envelope.sender)||envelope.sender===own)throw new Error('Invalid signal');
   if(host&&envelope.sender!==host)throw new Error('Unexpected peer');
   const data=envelope.data;if(!data||data.protocol!=='crabbridge-v1')throw new Error('Invalid protocol');
   if(data.type==='ready'){
    if(sent||Object.keys(data).sort().join(',')!=='protocol,type')throw new Error('Duplicate ready');host=envelope.sender;sent=true;
    ws.send(JSON.stringify({Signal:{receiver:host,data:{protocol:'crabbridge-v1',type:'offer',sdp:offer.sdp}}}));return;
   }
   if(data.type!=='answer'||!sent||Object.keys(data).sort().join(',')!=='protocol,sdp,type'||typeof data.sdp!=='string'||bytes(data.sdp)>65536||!data.sdp.startsWith('v=0\r\n')||data.sdp.includes('\0'))throw new Error('Invalid answer');
   finish(null,{type:'answer',sdp:data.sdp});
  }catch(error){finish(error);}};
 });
}
