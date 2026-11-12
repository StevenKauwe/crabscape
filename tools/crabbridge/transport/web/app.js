import init,{BrowserChannel} from './pkg/harbor_noise.js';
const $=id=>document.getElementById(id);let channel,pc,dc;
await init();$('status').textContent='Ready. Create an offer.';
$('offer').onclick=async()=>{try{
 if(pc)pc.close();if(channel)channel.free();channel=new BrowserChannel($('key').value,$('context').value);$('key').value='';
 pc=new RTCPeerConnection({iceServers:[]});dc=pc.createDataChannel('harbor-v1',{ordered:true});dc.binaryType='arraybuffer';
 dc.onopen=()=>{dc.send(channel.begin());$('status').textContent='Authenticating…';};
 dc.onmessage=event=>{try{const data=new Uint8Array(event.data);if(!channel.ready()){channel.finish(data);$('send').disabled=false;$('status').textContent='Noise established. Send echo to confirm round-trip possession.';}else{const message=JSON.parse(new TextDecoder().decode(channel.decrypt(data)));$('status').textContent=JSON.stringify(message);}}catch(e){$('status').textContent='Authentication/decryption failed';dc.close();$('send').disabled=true;}};
 dc.onclose=()=>{$('send').disabled=true;$('status').textContent+='\nChannel closed.';};
 await pc.setLocalDescription(await pc.createOffer());await new Promise((resolve,reject)=>{if(pc.iceGatheringState==='complete')return resolve();const timer=setTimeout(()=>reject(new Error('ICE timeout')),10000);pc.onicegatheringstatechange=()=>{if(pc.iceGatheringState==='complete'){clearTimeout(timer);resolve();}};});
 $('offer-json').value=JSON.stringify(pc.localDescription);$('status').textContent='Offer ready. Transfer to local host and paste answer.';
}catch(e){$('status').textContent=String(e);}};
$('connect').onclick=async()=>{try{const answer=JSON.parse($('answer-json').value);await pc.setRemoteDescription(answer);}catch(e){$('status').textContent=String(e);}};
$('send').onclick=()=>{try{const text=$('text').value;if(text.length>4096)throw new Error('Too long');const plain=new TextEncoder().encode(JSON.stringify({type:'echo',text}));dc.send(channel.encrypt(plain));}catch(e){$('status').textContent=String(e);}};
