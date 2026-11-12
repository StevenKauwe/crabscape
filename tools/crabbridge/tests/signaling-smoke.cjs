const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const fs=require('fs'),path=require('path'),cp=require('child_process');
const root=path.resolve(__dirname,'..'),bin=path.join(root,'target/debug/codex-harbor');
const brokerBin=path.join(root,'transport/target/debug/harbor-rendezvous');
const run=fs.mkdtempSync('/tmp/harbor-signaling-proof-'),pause=ms=>new Promise(r=>setTimeout(r,ms));
let browser,server,broker,host;
async function stop(child){if(!child||child.exitCode!==null)return;child.kill('SIGINT');await new Promise(resolve=>child.once('exit',resolve));}
async function ready(child,pattern){let output='';child.stdout.on('data',b=>output+=b);child.stderr.on('data',b=>output+=b);for(let i=0;i<100;i++){if(pattern.test(output))return;if(child.exitCode!==null)throw new Error('Fixture process exited before ready: '+output);await pause(100);}throw new Error('Fixture readiness timeout');}
(async()=>{try{
 const config=path.join(run,'config.json'),registration=path.join(run,'cat.json'),pairing=path.join(run,'pairing');
 cp.execFileSync(bin,['init',config]);fs.writeFileSync(registration,JSON.stringify({executable:'/bin/cat',args:[],workspace:run}));cp.execFileSync(bin,['register',config,'cat',registration]);cp.execFileSync(bin,['add-user',config,'tester','control','cat','--password-stdin'],{input:'signaling-test-password-123'});
 cp.execFileSync(bin,['remote-prepare',config,'tester',pairing]);
 if(fs.readFileSync(path.join(pairing,'.gitignore'),'utf8')!=='*\n')throw new Error('Pairing credentials lack accidental-commit guard');
 const invitePath=path.join(pairing,'invite.json'),invite=JSON.parse(fs.readFileSync(invitePath)),brokerConfig=JSON.parse(fs.readFileSync(path.join(pairing,'signaling.json')));
 if(JSON.stringify(brokerConfig).includes(invite.key)||JSON.stringify(brokerConfig).includes('signaling-test-password'))throw new Error('Application credential leaked into broker config');
 server=cp.spawn(bin,['serve',config]);await ready(server,/Local security prototype/);
 broker=cp.spawn(brokerBin,[path.join(pairing,'signaling.json')]);await ready(broker,/Loopback signaling ready/);
 host=cp.spawn(bin,['remote-listen',config,invitePath,path.join(pairing,'host.json')]);
 browser=await chromium.launch({headless:true,...(process.env.CHROME_BIN?{executablePath:process.env.CHROME_BIN}:{})});
 const p=await browser.newPage({viewport:{width:390,height:844}}),requests=[],errors=[],urls=[];
 p.on('pageerror',e=>errors.push(String(e)));p.on('request',r=>{urls.push(r.url());if(r.url().startsWith('http://127.0.0.1:7879'))requests.push({url:r.url(),body:r.postData()||'',headers:r.headers()});});
 await p.goto('http://127.0.0.1:7878/remote.html');await p.waitForFunction(()=>document.querySelector('#status').textContent.startsWith('Ready.'));
 await p.setInputFiles('#invite',invitePath);await p.waitForFunction(()=>!document.querySelector('#offer').disabled);await p.click('#offer');
 await p.waitForFunction(()=>!document.querySelector('#login').disabled,{},{timeout:30000});
 if(await p.inputValue('#offer-json'))throw new Error('Automatic flow fell back to manual offer');
 await p.fill('#password','signaling-test-password-123');await p.click('#login');await p.waitForFunction(()=>document.querySelector('#tool').options.length===1);await p.click('#start');await p.waitForFunction(()=>document.querySelector('#status').textContent.includes('running'));
 await p.locator('.xterm-helper-textarea').pressSequentially('automated-signaling-pty-proof');await p.locator('.xterm-helper-textarea').press('Enter');await p.waitForFunction(()=>document.querySelector('.xterm-rows').textContent.includes('automated-signaling-pty-proof'));
 await p.click('#stop');await p.waitForFunction(()=>document.querySelector('#status').textContent.includes('Stop requested'));await p.click('#logout');await p.waitForFunction(()=>document.querySelector('#tool').options.length===0);
 for(let i=0;i<100&&host.exitCode===null;i++)await pause(100);if(host.exitCode===null)throw new Error('Host did not close');
 const captured=JSON.stringify(requests);for(const secret of [invite.key,brokerConfig.host_token,'signaling-test-password-123','automated-signaling-pty-proof'])if(captured.includes(secret))throw new Error('Application secret/input appeared in browser signaling');
 if(!requests.some(r=>r.url.endsWith('/offer')&&r.body)||!requests.some(r=>r.url.endsWith('/answer')))throw new Error('Signaling endpoints were not exercised');
 if(errors.length)throw new Error(errors.join('\n'));
 // A consumed signaling room cannot be silently reused after logout.
 await p.setInputFiles('#invite',invitePath);await p.waitForFunction(()=>!document.querySelector('#offer').disabled);await p.click('#offer');await p.waitForFunction(()=>document.querySelector('#status').textContent.includes('Operation failed'));
 // Reject an invitation-selected endpoint locally, before any destination request.
 const bad=path.join(run,'bad-invite.json');fs.writeFileSync(bad,JSON.stringify({...invite,signal:{...invite.signal,endpoint:'http://127.0.0.1:7880'}}));
 await p.setInputFiles('#invite',bad);await p.waitForFunction(()=>document.querySelector('#status').textContent.includes('Operation failed'));
 if(!(await p.locator('#offer').isDisabled()))throw new Error('Rejected endpoint left pairing enabled');
 if(urls.some(url=>url.startsWith('http://127.0.0.1:7880')))throw new Error('Rejected endpoint was contacted');
 console.log('PASS: automatic local signaling→Noise/WebRTC→password→registered PTY; no application secret/input in browser signaling; hostile endpoint rejected.');
 }finally{if(browser)await browser.close();if(host&&host.exitCode===null){for(let i=0;i<30&&host.exitCode===null;i++)await pause(100);await stop(host);}await stop(broker);await stop(server);fs.rmSync(run,{recursive:true,force:true});}
})().catch(e=>{console.error(e);process.exit(1)});
