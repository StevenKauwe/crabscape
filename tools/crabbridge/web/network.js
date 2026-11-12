// Only a host-selected, Noise-context-bound profile may enable external discovery.
// SDP filtering is not a firewall; ordinary WebRTC may accept peer-reflexive traffic.
export function networkConfiguration(context) {
  if (!context || typeof context !== 'object' || Array.isArray(context)) throw new Error('Invalid peer context');
  if (context.network_profile === undefined || context.network_profile === null) return {iceServers:[]};
  if (context.network_profile !== 'direct-stun-v1') throw new Error('Unsupported network profile');
  return {iceServers:[{urls:['stun:stun.l.google.com:19302','stun:stun1.l.google.com:19302']}],iceTransportPolicy:'all'};
}
export function networkNotice(context) {
  networkConfiguration(context);
  return context.network_profile === 'direct-stun-v1'
    ? 'Direct Internet mode: Google STUN discovers network addresses; CrabScape signaling receives connection metadata. No TURN relay; some mobile networks will not connect.'
    : 'Loopback or explicitly approved LAN pairing; no external ICE discovery.';
}
function ipv4(text){const parts=text.split('.');return parts.length===4&&parts.every(x=>/^(0|[1-9][0-9]{0,2})$/.test(x)&&Number(x)<=255)?parts.map(Number):null;}
function publicV4(text){const p=ipv4(text);if(!p)return false;const[a,b,c]=p;return !(a===0||a===10||a===127||a>=224||(a===100&&b>=64&&b<=127)||(a===169&&b===254)||(a===172&&b>=16&&b<=31)||(a===192&&(b===168||(b===0&&(c===0||c===2))||(b===88&&c===99)))||(a===198&&(b===18||b===19||(b===51&&c===100)))||(a===203&&b===0&&c===113));}
function uint(text,max){return /^[0-9]+$/.test(text)&&Number.isSafeInteger(Number(text))&&Number(text)<=max;}
export function directCandidate(line){
 if(new TextEncoder().encode(line).length>1024||!line.startsWith('a=candidate:'))return false;
 const p=line.slice(12).trim().split(/\s+/);if(p.length<8||p.length>32||!(/^[a-zA-Z0-9]{1,32}$/).test(p[0])||p[1]!=='1'||p[2].toLowerCase()!=='udp'||!uint(p[3],4294967295)||!publicV4(p[4])||!uint(p[5],65535)||Number(p[5])<1024||p[6]!=='typ'||!['host','srflx'].includes(p[7])||(p.length-8)%2)return false;
 const seen=new Set();for(let i=8;i<p.length;i+=2){const k=p[i],v=p[i+1];if(seen.has(k))return false;seen.add(k);if(k==='raddr'){if(!ipv4(v))return false;}else if(k==='rport'){if(!uint(v,65535))return false;}else if(['generation','network-id','network-cost'].includes(k)){if(!uint(v,4294967295))return false;}else if(k==='ufrag'){if(!(/^[a-zA-Z0-9+/]{1,256}$/).test(v))return false;}else return false;}return true;
}
export function validateNetworkSdp(context,sdp){
 networkConfiguration(context);if(context.network_profile!=='direct-stun-v1')return;
 if(typeof sdp!=='string'||new TextEncoder().encode(sdp).length>65536)throw new Error('SDP bounds');
 let count=0;for(const line of sdp.split(/\r?\n/)){if(new TextEncoder().encode(line).length>2048)throw new Error('SDP line bounds');if(line.startsWith('a=candidate:')){if(!directCandidate(line))throw new Error('Unsupported direct candidate');count++;}}
 if(count<1||count>16)throw new Error('Direct mode needs 1..16 public IPv4 UDP candidates; this network may need TURN.');
}
export function publicOffer(context,description){
 networkConfiguration(context);if(context.network_profile!=='direct-stun-v1')return{type:description.type,sdp:description.sdp};
 const sdp=description.sdp.split(/\r?\n/).filter(x=>x&&!x.startsWith('a=candidate:')||x.startsWith('a=candidate:')&&directCandidate(x)).join('\r\n')+'\r\n';validateNetworkSdp(context,sdp);return{type:description.type,sdp};
}

// Validate the finite grant immediately before constructing a network-capable peer.
export function createPeer(context, Peer = globalThis.RTCPeerConnection, now = Math.floor(Date.now()/1000)) {
  const expiry=context?.expires_at;
  if(!Number.isSafeInteger(expiry)||!Number.isSafeInteger(now)||expiry<=now||expiry>now+600)throw new Error('Invitation expired or outside the ten-minute grant window');
  return new Peer(networkConfiguration(context));
}
