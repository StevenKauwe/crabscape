// Explicit feature-enabled protocol fixture. No real credentials, DNS/TLS changes or deployment.
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const fs=require('fs'),path=require('path'),cp=require('child_process');
const root=path.resolve(__dirname,'..');
const bin=process.env.HARBOR_PASSKEY_BIN;
if(!bin)throw Error('Set HARBOR_PASSKEY_BIN to an explicitly feature-built test binary');
const run=fs.mkdtempSync('/tmp/crabbridge-passkey-proof-');fs.chmodSync(run,0o700);
const config=path.join(run,'config.json'),password='only-disposable-virtual-passkey-fixture';
const origin='https://bridge.example.test',pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
let browser,host,cdp,authenticatorId,counter=0;const children=[];
function command(args,input){return cp.execFileSync(bin,args,{input,encoding:'utf8'});}
async function stopHost(){if(!host)return;const child=host;host=null;if(child.exitCode!==null||child.signalCode!==null)return;child.kill('SIGINT');await Promise.race([new Promise(resolve=>child.once('exit',resolve)),pause(5000).then(()=>{if(child.exitCode===null)child.kill('SIGKILL');})]);}
(async()=>{try{
 command(['init',config]);const registration=path.join(run,'cat.json');fs.writeFileSync(registration,JSON.stringify({executable:'/bin/cat',args:[],workspace:run}),{mode:0o600});command(['register',config,'cat',registration]);command(['add-user',config,'tester','control','cat','--password-stdin'],password);
 command(['device-init',config,origin,'bridge.example.test',path.join(run,'devices')]);
 const policy=await import('data:text/javascript;base64,'+fs.readFileSync(path.join(root,'web/deployment.js')).toString('base64'));
 browser=await chromium.launch({headless:true,chromiumSandbox:true,...(process.env.CHROME_BIN?{executablePath:process.env.CHROME_BIN}:{})});
 const context=await browser.newContext({viewport:{width:390,height:844}}),unexpected=[];
 const files={'/remote.html':'web/remote.html','/remote.js':'web/remote.js','/deployment.js':'web/deployment.js','/matchbox.js':'web/matchbox.js','/style.css':'web/style.css','/vendor/xterm.js':'web/vendor/xterm.js','/vendor/xterm.css':'web/vendor/xterm.css','/remote/pkg/harbor_noise.js':'transport/web/pkg/harbor_noise.js','/remote/pkg/harbor_noise_bg.wasm':'transport/web/pkg/harbor_noise_bg.wasm'};
 await context.route('**/*',async route=>{const request=route.request(),url=new URL(request.url()),file=files[url.pathname];if(url.origin!==origin||!file||!['GET','HEAD'].includes(request.method())){unexpected.push(url.origin+url.pathname);return route.abort();}const headers={'Content-Security-Policy':policy.PRODUCTION_CSP,'X-Frame-Options':'DENY','Cache-Control':'no-store','Referrer-Policy':'no-referrer','X-Content-Type-Options':'nosniff','Permissions-Policy':'camera=(), microphone=(), geolocation=()','X-CrabBridge-Policy':'1'};headers['Content-Type']=file.endsWith('.wasm')?'application/wasm':file.endsWith('.js')?'text/javascript':file.endsWith('.css')?'text/css':'text/html';await route.fulfill({status:200,headers,body:request.method()==='HEAD'?'':fs.readFileSync(path.join(root,file))});});
 let page=await context.newPage();
 // Track actual xterm buffers rather than DOM render scheduling.
 await page.addInitScript(()=>{let constructor;window.__fixtureTerminals=[];Object.defineProperty(window,'Terminal',{configurable:true,get(){return constructor;},set(value){constructor=new Proxy(value,{construct(target,args){const terminal=Reflect.construct(target,args);window.__fixtureTerminals.push(terminal);return terminal;}});}}); });
 cdp=await context.newCDPSession(page);await cdp.send('WebAuthn.enable');({authenticatorId}=await cdp.send('WebAuthn.addVirtualAuthenticator',{options:{protocol:'ctap2',transport:'internal',hasResidentKey:true,hasUserVerification:true,isUserVerified:true,automaticPresenceSimulation:true}}));
 async function pair(){await stopHost();const n=++counter,invite=path.join(run,`invite-${n}.json`),offer=path.join(run,`offer-${n}.json`),answer=path.join(run,`answer-${n}.json`);command(['remote-invite',config,'tester',invite]);await page.goto(origin+'/remote.html');await page.waitForFunction(()=>document.querySelector('#status').textContent.startsWith('Ready.'));if(await page.evaluate(()=>document.documentElement.scrollWidth>window.innerWidth))throw Error('Mobile page overflows viewport');await page.setInputFiles('#invite',invite);await page.waitForFunction(()=>!document.querySelector('#offer').disabled);await page.click('#offer');await page.waitForFunction(()=>document.querySelector('#offer-json').value.length>0);fs.writeFileSync(offer,await page.inputValue('#offer-json'),{mode:0o600});host=cp.spawn(bin,['remote-answer',config,invite,offer,answer],{stdio:['ignore','ignore','ignore']});children.push(host);for(let i=0;i<200&&!fs.existsSync(answer);i++){if(host.exitCode!==null)throw Error('Peer exited before answer');await pause(50);}if(!fs.existsSync(answer))throw Error('Peer answer timeout');await page.fill('#answer-json',fs.readFileSync(answer,'utf8'));await page.click('#connect');await page.waitForFunction(()=>!document.querySelector('#passkey-login').disabled);}
 async function loginPassword(){await page.fill('#password',password);await page.click('#login');await page.waitForFunction(()=>document.querySelector('#tool').options.length===1);}
 async function logout(){await page.click('#logout');await page.waitForFunction(()=>document.querySelector('#status').textContent.startsWith('Logged out.'));await stopHost();}
 await pair();await loginPassword();await page.fill('#password',password);await page.click('#passkey-register');await page.waitForFunction(()=>document.querySelector('#status').textContent.startsWith('Passkey registered.'));await page.click('#devices-refresh');await page.waitForFunction(()=>document.querySelector('#device-list').options.length===1);await logout();
 let replayAssertion;
 for(const variant of ['userHandle','challenge','origin','replay']){
  await pair();
  await page.evaluate(({variant,replay})=>{
   const get=navigator.credentials.get.bind(navigator.credentials);
   const encode=value=>btoa(String.fromCharCode(...new Uint8Array(value))).replace(/\+/g,'-').replace(/\//g,'_').replace(/=+$/,'');
   const decode=value=>Uint8Array.from(atob(value.replace(/-/g,'+').replace(/_/g,'/')),char=>char.charCodeAt(0)).buffer;
   navigator.credentials.get=async options=>{
    const credential=await get(options),response={clientDataJSON:credential.response.clientDataJSON,authenticatorData:credential.response.authenticatorData,signature:credential.response.signature,userHandle:credential.response.userHandle};
    window.__fixtureCapturedAssertion={id:credential.id,rawId:encode(credential.rawId),response:Object.fromEntries(Object.entries(response).map(([key,value])=>[key,value===null?null:encode(value)]))};
    let id=credential.id,rawId=credential.rawId;
    if(variant==='userHandle')response.userHandle=new Uint8Array(16).fill(255).buffer;
    if(variant==='challenge'||variant==='origin'){const data=JSON.parse(new TextDecoder().decode(response.clientDataJSON));data[variant]=variant==='origin'?'https://different.example.test':'Zm9yZWlnbi1jaGFsbGVuZ2U';response.clientDataJSON=new TextEncoder().encode(JSON.stringify(data)).buffer;}
    if(variant==='replay'){id=replay.id;rawId=decode(replay.rawId);for(const [key,value]of Object.entries(replay.response))response[key]=value===null?null:decode(value);}
    return{id,rawId,type:credential.type,authenticatorAttachment:credential.authenticatorAttachment,getClientExtensionResults:()=>credential.getClientExtensionResults(),response};
   };
  },{variant,replay:replayAssertion});
  await page.click('#passkey-login');await page.waitForFunction(()=>(/failed|disconnected|closed/i).test(document.querySelector('#status').textContent));
  if(await page.locator('#tool option').count())throw Error(`${variant} assertion obtained tools`);
  if(!replayAssertion)replayAssertion=await page.evaluate(()=>window.__fixtureCapturedAssertion);
  console.log(`Rejected valid-credential ${variant} variant without tool access`);
 }
 await pair();await page.click('#passkey-login');await page.waitForFunction(()=>document.querySelector('#tool').options.length===1);await page.selectOption('#tool','cat');await page.click('#start');await page.waitForFunction(()=>document.querySelector('#status').textContent.includes('running'));await page.locator('.xterm-helper-textarea').pressSequentially('VIRTUAL_PASSKEY_AUTHORIZED_TOOL');await page.locator('.xterm-helper-textarea').press('Enter');await page.waitForFunction(()=>{const buffer=window.__fixtureTerminals.at(-1).buffer.active;for(let i=0;i<buffer.length;i++)if(buffer.getLine(i)?.translateToString().includes('VIRTUAL_PASSKEY_AUTHORIZED_TOOL'))return true;return false;});await page.click('#stop');await page.waitForFunction(()=>document.querySelector('#status').textContent.startsWith('Stop requested'));
 const devicePage=page;host=null;page=await context.newPage();
 await pair();await loginPassword();await page.click('#devices-refresh');await page.waitForFunction(()=>document.querySelector('#device-list').options.length===1);await page.click('#device-remove');await page.waitForFunction(()=>document.querySelector('#status').textContent==='Selected passkey removed.');if(await page.locator('#device-list option').count())throw Error('Revoked device still offered as active');
 await devicePage.waitForFunction(()=>document.querySelector('#status').textContent.startsWith('Peer disconnected.'));if(await devicePage.locator('#tool option').count())throw Error('Idle revoked device retained tools');
 await logout();await page.close();page=devicePage;
 await pair();await page.click('#passkey-login');await page.waitForFunction(()=>(/failed|disconnected|closed/i).test(document.querySelector('#status').textContent));if(await page.locator('#tool option').count())throw Error('Revoked credential obtained tools');
 // Browser cancellation (same epoch) closes the peer rather than pretending server state was cleared.
 await pair();await loginPassword();await page.evaluate(()=>{navigator.credentials.create=async()=>{throw new DOMException('Fixture user cancellation','NotAllowedError');};});await page.fill('#password',password);await page.click('#passkey-register');await page.waitForFunction(()=>document.querySelector('#status').textContent.startsWith('Passkey operation cancelled.'));if(await page.locator('#tool option').count())throw Error('Cancelled prompt retained session');await stopHost();
 // Cancel a genuine pending virtual-authenticator ceremony by disconnecting.
 await pair();await loginPassword();
 await cdp.send('WebAuthn.setAutomaticPresenceSimulation',{authenticatorId,enabled:false});
 await page.evaluate(()=>{window.__fixtureCredentialStarted=false;const create=navigator.credentials.create.bind(navigator.credentials);navigator.credentials.create=options=>{window.__fixtureCredentialStarted=true;return create(options);};});
 await page.fill('#password',password);await page.click('#passkey-register');await page.waitForFunction(()=>window.__fixtureCredentialStarted===true);await page.click('#disconnect');
 await page.waitForFunction(()=>document.querySelector('#status').textContent.startsWith('Disconnected.'));await stopHost();
 const devices=JSON.parse(command(['device-list',config,'tester']));if(devices.some(device=>!device.revoked))throw Error('Cancelled registration committed a new device');
 // A completed browser result delivered AFTER reset must never reach the host finish RPC.
 await pair();await loginPassword();await cdp.send('WebAuthn.setAutomaticPresenceSimulation',{authenticatorId,enabled:true});
 await page.evaluate(()=>{window.__fixtureResultHeld=false;const create=navigator.credentials.create.bind(navigator.credentials);navigator.credentials.create=async options=>{const result=await create(options);window.__fixtureResultHeld=true;return new Promise(resolve=>{window.__fixtureReleaseCredential=()=>resolve(result);});};});
 await page.fill('#password',password);await page.click('#passkey-register');await page.waitForFunction(()=>window.__fixtureResultHeld===true);await page.click('#disconnect');await page.evaluate(()=>window.__fixtureReleaseCredential());await stopHost();
 if(JSON.parse(command(['device-list',config,'tester'])).some(device=>!device.revoked))throw Error('Late credential completion survived reset');
 // Retrying requires a fresh pairing and can then enroll normally.
 await pair();await loginPassword();await page.fill('#password',password);await page.click('#passkey-register');await page.waitForFunction(()=>document.querySelector('#status').textContent.startsWith('Passkey registered.'));await logout();
 if(unexpected.length)throw Error('Unexpected outbound requests: '+unexpected.join(','));
 console.log('PASS: virtual passkey enrollment, fresh-peer passwordless login, real authorized PTY, idle-device revocation, subsequent denial and disconnected pending-ceremony cancellation; 390px mobile layout; exact reserved HTTPS RP; no external requests.');
 }catch(error){if(browser){for(const context of browser.contexts())for(const page of context.pages())try{console.error('SANITIZED_PAGE_STATE',await page.locator('#status').textContent());}catch{}}console.error('PEER_PROCESS_EXIT_CODES',children.map(child=>child.exitCode));throw error;}finally{if(cdp&&authenticatorId)try{await cdp.send('WebAuthn.removeVirtualAuthenticator',{authenticatorId});}catch{}if(browser)await browser.close();await stopHost();for(const child of children){if(child.exitCode===null&&child.signalCode===null){host=child;await stopHost();}}fs.rmSync(run,{recursive:true,force:true});}
})().catch(error=>{console.error(error);process.exitCode=1;});
