const assert=require('node:assert/strict'),fs=require('node:fs');
(async()=>{
 const source=fs.readFileSync(require('node:path').join(__dirname,'../web/matchbox.js'),'utf8');const {matchboxAnswer}=await import('data:text/javascript;base64,'+Buffer.from(source).toString('base64'));
 let contacts=0;global.WebSocket=class{constructor(){contacts++;throw new Error('No contact allowed');}};
 const signal={endpoint:'ws://127.0.0.1:3536',room:'crabscape-crabbridge-'+'a'.repeat(32)},expires=Math.floor(Date.now()/1000)+60,abort=new AbortController().signal;
 for(const offer of [null,{}, {type:'answer',sdp:'v=0\r\n'}, {type:'offer',sdp:'v=0\r\n',extra:true}, {type:'offer',sdp:'v=0\r\n'+'x'.repeat(65536)}, {type:'offer',sdp:'v=0\r\n'+'é'.repeat(32768)}, {type:'offer',sdp:'v=0\r\n\0'}, {type:'offer',sdp:'bad'}])assert.throws(()=>matchboxAnswer(signal,offer,expires,abort));
 for(const endpoint of ['ws://evil.invalid','ws://localhost:3536','wss://crabscape-introducer.fly.dev/extra'])assert.throws(()=>matchboxAnswer({...signal,endpoint},{type:'offer',sdp:'v=0\r\n'},expires,abort));
 assert.equal(contacts,0);console.log('PASS: invalid/oversized offers and destinations rejected before any WebSocket contact.');
})().catch(e=>{console.error(e);process.exit(1)});
