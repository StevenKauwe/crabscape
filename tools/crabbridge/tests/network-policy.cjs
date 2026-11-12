const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
(async()=>{
 const source=fs.readFileSync(path.join(__dirname,'../web/network.js'),'utf8');
 const {createPeer,networkConfiguration,networkNotice,directCandidate,publicOffer,validateNetworkSdp}=await import('data:text/javascript;base64,'+Buffer.from(source).toString('base64'));
 assert.deepEqual(networkConfiguration({}),{iceServers:[]});
 const expected={iceServers:[{urls:['stun:stun.l.google.com:19302','stun:stun1.l.google.com:19302']}],iceTransportPolicy:'all'};
 assert.deepEqual(networkConfiguration({network_profile:'direct-stun-v1'}),expected);
 for(const profile of ['turn','http://example.com',{},[],true,1])assert.throws(()=>networkConfiguration({network_profile:profile}));
 for(const context of [null,[],false])assert.throws(()=>networkConfiguration(context));
 assert.match(networkNotice({network_profile:'direct-stun-v1'}),/No TURN relay/);
 const c={network_profile:'direct-stun-v1'}, good='a=candidate:1 1 UDP 123 93.184.216.34 12345 typ srflx raddr 0.0.0.0 rport 0';
 assert(directCandidate(good));
 for(const ip of ['127.0.0.1','10.0.0.1','100.64.0.1','169.254.1.1','172.16.0.1','192.168.1.1','192.0.2.1','198.51.100.1','203.0.113.1','224.0.0.1','example.local','::1'])assert(!directCandidate(good.replace('93.184.216.34',ip)));
 for(const change of [good.replace('UDP','TCP'),good.replace('srflx','relay'),good+' generation 0 generation 1',good.replace('12345','80')])assert(!directCandidate(change));
 const filtered=publicOffer(c,{type:'offer',sdp:'v=0\r\n'+good+'\r\na=candidate:2 1 UDP 123 10.0.0.1 12345 typ host\r\n'});assert(!filtered.sdp.includes('10.0.0.1'));validateNetworkSdp(c,filtered.sdp);
 assert.throws(()=>publicOffer(c,{type:'offer',sdp:'v=0\r\n'}));assert.throws(()=>validateNetworkSdp(c,'v=0\r\n'+Array(17).fill(good).join('\r\n')));
 let contacts=0;class FakePeer{constructor(config){contacts++;this.config=config;}}
 for(const expiry of [undefined,null,NaN,Infinity,1000,999,1601,1000.5,'1200'])assert.throws(()=>createPeer({...c,expires_at:expiry},FakePeer,1000));
 assert.equal(contacts,0,'invalid grants must not construct a peer or start discovery');
 assert.deepEqual(createPeer({...c,expires_at:1600},FakePeer,1000).config,expected);assert.equal(contacts,1);
 console.log('PASS: context-bound network profile rejects arbitrary destinations; default has no ICE servers. No network contacted.');
})().catch(e=>{console.error(e);process.exit(1)});
