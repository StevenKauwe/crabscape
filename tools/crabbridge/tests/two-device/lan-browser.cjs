const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const fs=require('fs'),path=require('path'),http=require('http');
const dir=process.env.HARBOR_FIXTURE_DIR||__dirname,assets=process.argv[2],pause=ms=>new Promise(r=>setTimeout(r,ms));
const clientIP=process.env.HARBOR_CLIENT_IP||'192.168.1.198',hostIP=process.env.HARBOR_HOST_IP||'192.168.1.147';
let browser,server,page;
const routes={
 '/':'remote.html','/remote.html':'remote.html','/remote.js':'remote.js','/matchbox.js':'matchbox.js','/style.css':'style.css',
 '/vendor/xterm.js':'vendor/xterm.js','/vendor/xterm.css':'vendor/xterm.css',
 '/remote/pkg/harbor_noise.js':'remote/pkg/harbor_noise.js','/remote/pkg/harbor_noise_bg.wasm':'remote/pkg/harbor_noise_bg.wasm'
};
(async()=>{try{
 if(!assets)throw new Error('Verified frontend asset root required');
 for(const relative of Object.values(routes))if(!fs.statSync(path.join(assets,relative)).isFile())throw new Error('Missing trusted asset');
 server=http.createServer((req,res)=>{
  if(req.headers.host!=='127.0.0.1:7878'||req.method!=='GET'||!routes[req.url]){res.writeHead(404);return res.end();}
  const file=path.join(assets,routes[req.url]),ext=path.extname(file);
  const type={'.html':'text/html; charset=utf-8','.js':'text/javascript; charset=utf-8','.css':'text/css; charset=utf-8','.wasm':'application/wasm'}[ext];
  res.writeHead(200,{'Content-Type':type,'Cache-Control':'no-store','X-Content-Type-Options':'nosniff','Referrer-Policy':'no-referrer','Content-Security-Policy':"default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'",'Permissions-Policy':'camera=(), microphone=(), geolocation=()'});fs.createReadStream(file).pipe(res);
 });
 await new Promise((resolve,reject)=>{server.once('error',reject);server.listen(7878,'127.0.0.1',resolve);});
 browser=await chromium.launch({headless:true,executablePath:process.env.CHROME_BIN||'/usr/bin/chromium',args:['--disable-features=WebRtcHideLocalIpsWithMdns']});
 page=await browser.newPage({viewport:{width:1200,height:900}});const errors=[];
 await page.addInitScript(()=>{let terminalClass;window.__harborTerminals=[];Object.defineProperty(window,'Terminal',{configurable:true,get(){return terminalClass;},set(value){terminalClass=new Proxy(value,{construct(target,args){const term=Reflect.construct(target,args);window.__harborTerminals.push(term);return term;}});}});const Native=window.RTCPeerConnection;window.__harborPeers=[];window.RTCPeerConnection=class extends Native{constructor(...args){super(...args);window.__harborPeers.push(this);}};});
 page.on('pageerror',e=>errors.push(String(e)));page.on('console',m=>{if(m.type()==='error')console.log('BROWSER_ERROR '+m.text());});
 await page.goto('http://127.0.0.1:7878/remote.html');await page.waitForFunction(()=>document.querySelector('#status').textContent.startsWith('Ready.'));
 console.log('LOCAL_NETWORK_PERMISSION '+JSON.stringify(await page.evaluate(async()=>{const result={};for(const name of ['local-network-access','local-network'])try{result[name]=(await navigator.permissions.query({name})).state;}catch{result[name]='unsupported';}return result;})));
 await page.setInputFiles('#invite',path.join(dir,'invite.json'));await page.waitForFunction(()=>!document.querySelector('#offer').disabled);await page.click('#offer');
 await page.waitForFunction(()=>document.querySelector('#offer-json').value.length>0,{},{timeout:20000});
 const offer=JSON.parse(await page.inputValue('#offer-json'));
 const candidates=offer.sdp.split('\r\n').filter(x=>x.startsWith('a=candidate:'));
 const allowed=candidates.filter(line=>{const p=line.split(' ');return p[2].toLowerCase()==='udp'&&p[4]===clientIP&&p[7]==='host';});
 if(!allowed.length)throw new Error('No actual numeric UDP host candidate for '+clientIP+': '+JSON.stringify(candidates));
 offer.sdp=offer.sdp.split('\r\n').filter(x=>!x.startsWith('a=candidate:')||allowed.includes(x)).join('\r\n');
 fs.writeFileSync(path.join(dir,'offer.json'),JSON.stringify(offer),{mode:0o600});
 console.log('OFFER_READY '+JSON.stringify({kept_candidates:allowed.length,addresses:[clientIP]}));
 const deadline=Date.now()+240000;
 while(!fs.existsSync(path.join(dir,'answer.json'))){if(Date.now()>deadline)throw new Error('Timed out waiting for host answer');await pause(100);}
 const answer=fs.readFileSync(path.join(dir,'answer.json'),'utf8');await page.fill('#answer-json',answer);await page.click('#connect');console.log('ANSWER_APPLIED '+new Date().toISOString());
 await page.waitForFunction(()=>!document.querySelector('#login').disabled,{},{timeout:100000});
 await page.fill('#password',fs.readFileSync(path.join(dir,'password.txt'),'utf8').trim());await page.click('#login');await page.waitForFunction(()=>document.querySelector('#tool').options.length>0);
 await page.selectOption('#tool','cat');await page.click('#start');await page.waitForFunction(()=>document.querySelector('#status').textContent.includes('running'));
 await page.locator('.xterm-helper-textarea').pressSequentially('REAL_TWO_DEVICE_LAN_PROOF');await page.locator('.xterm-helper-textarea').press('Enter');await page.waitForFunction(()=>(()=>{const term=window.__harborTerminals.at(-1),buffer=term?.buffer.active;if(!buffer)return false;for(let i=0;i<buffer.length;i++)if(buffer.getLine(i)?.translateToString().includes('REAL_TWO_DEVICE_LAN_PROOF'))return true;return false;})());
 const pair=await page.evaluate(async()=>{const stats=await window.__harborPeers[0].getStats();let chosen;stats.forEach(s=>{if(s.type==='transport'&&s.selectedCandidatePairId)chosen=stats.get(s.selectedCandidatePairId);});if(!chosen)stats.forEach(s=>{if(s.type==='candidate-pair'&&s.state==='succeeded'&&s.nominated)chosen=s;});if(!chosen)return null;const a=stats.get(chosen.localCandidateId),b=stats.get(chosen.remoteCandidateId);return{state:chosen.state,local:{address:a.address,port:a.port,protocol:a.protocol,type:a.candidateType},remote:{address:b.address,port:b.port,protocol:b.protocol,type:b.candidateType}};});
 if(!pair||pair.local.address!==clientIP||pair.remote.address!==hostIP||pair.local.protocol!=='udp')throw new Error('Selected peer pair not the authorized two-device LAN path: '+JSON.stringify(pair));
 await page.click('#stop');await page.waitForFunction(()=>document.querySelector('#status').textContent.includes('Stop requested'));await page.click('#logout');await page.waitForFunction(()=>document.querySelector('#tool').options.length===0);
 if(errors.length)throw new Error(errors.join('\n'));
 const result={pass:true,selected_pair:pair,marker_received:true,stopped_and_logged_out:true};fs.writeFileSync(path.join(dir,'result.json'),JSON.stringify(result,null,2));console.log('LAN_PROOF_PASS '+JSON.stringify(result));
 }catch(error){
 let diagnostics=null;
 if(page)try{diagnostics=await page.evaluate(async()=>{const peers=[];for(const pc of window.__harborPeers||[]){const stats=await pc.getStats(),records=[];stats.forEach(s=>{if(['candidate-pair','local-candidate','remote-candidate','transport'].includes(s.type)){const record={};for(const key of ['id','type','state','nominated','address','ip','port','protocol','candidateType','localCandidateId','remoteCandidateId','selectedCandidatePairId','requestsSent','responsesReceived','requestsReceived','responsesSent','bytesSent','bytesReceived','dtlsState','iceState'])if(s[key]!==undefined)record[key]=s[key];records.push(record);}});peers.push({connectionState:pc.connectionState,iceConnectionState:pc.iceConnectionState,iceGatheringState:pc.iceGatheringState,signalingState:pc.signalingState,stats:records});}return{uiStatus:document.querySelector('#status')?.textContent,peers};});}catch{}
 fs.writeFileSync(path.join(dir,'result.json'),JSON.stringify({pass:false,error:String(error),diagnostics},null,2));console.error(error);if(diagnostics)console.log('SANITIZED_DIAGNOSTICS '+JSON.stringify(diagnostics));process.exitCode=1;}

 finally{if(browser)await browser.close();if(server)await new Promise(resolve=>server.close(resolve));for(const name of ['invite.json','password.txt','offer.json','answer.json'])fs.rmSync(path.join(dir,name),{force:true});console.log('CLIENT_CLEANUP_COMPLETE');}
})();
