const{chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const fs=require('fs'),path=require('path'),cp=require('child_process');
const root=path.resolve(__dirname,'..'),bin=path.join(root,'target/debug/codex-harbor'),run=fs.mkdtempSync('/tmp/harbor-terminal-proof-');
const pause=ms=>new Promise(r=>setTimeout(r,ms));let browser,host,server;
(async()=>{try{
 const config=path.join(run,'config.json'),invite=path.join(run,'invite.json'),offer=path.join(run,'offer.json'),answer=path.join(run,'answer.json');cp.execFileSync(bin,['init',config]);
 const registration=path.join(run,'cat.json');fs.writeFileSync(registration,JSON.stringify({executable:'/bin/cat',args:[],workspace:run}));cp.execFileSync(bin,['register',config,'cat',registration]);
 let tools='cat';if(process.env.HARBOR_TEST_TOOL==='codex'){if(!process.env.CODEX_BIN)throw new Error('CODEX_BIN required for optional real-Codex prompt test');const reg=path.join(run,'codex.json');fs.writeFileSync(reg,JSON.stringify({executable:process.env.CODEX_BIN,args:['--no-daemon','--no-alt-screen'],workspace:root}));cp.execFileSync(bin,['register',config,'codex',reg]);tools+=',codex';}
 cp.execFileSync(bin,['add-user',config,'tester','control',tools,'--password-stdin'],{input:'only-a-test-password-123'});
 server=cp.spawn(bin,['serve',config],{stdio:['ignore','pipe','pipe']});let started=false,serverError='';server.stdout.on('data',()=>started=true);server.stderr.on('data',b=>serverError+=b);for(let i=0;i<50&&!started;i++){if(server.exitCode!==null)throw new Error('HTTP test server failed (port 7878 must be free): '+serverError);await pause(100);}if(!started)throw new Error('HTTP test server not ready');
 cp.execFileSync(bin,['remote-invite',config,'tester',invite]);
 browser=await chromium.launch({headless:true,...(process.env.CHROME_BIN?{executablePath:process.env.CHROME_BIN}:{})});const local=await browser.newPage({viewport:{width:390,height:844}});await local.goto('http://127.0.0.1:7878');await local.fill('#username','tester');await local.fill('#password','only-a-test-password-123');await local.click('#login');await local.waitForFunction(()=>document.querySelector('#tool').options.length>0);await local.selectOption('#tool','cat');await local.click('#start');await local.fill('#input','local-terminal-proof');await local.click('#send');await local.waitForFunction(()=>document.querySelector('.xterm-rows').textContent.includes('local-terminal-proof'));await local.click('#stop');await local.click('#logout');await local.waitForFunction(()=>document.querySelector('#result').textContent==='Session revoked.');if((await local.locator('.xterm-rows').textContent()).includes('local-terminal-proof'))throw new Error('Old output survived logout');await local.close();console.log('Local 390px browser PTY flow and logout screen clearing passed');
 const p=await browser.newPage();const errors=[];p.on('pageerror',e=>errors.push(String(e)));
 await p.goto('http://127.0.0.1:7878/remote.html');await p.waitForFunction(()=>document.querySelector('#status').textContent.startsWith('Ready.'));
 await p.setInputFiles('#invite',invite);await p.waitForFunction(()=>document.querySelector('#identity').textContent.includes('tester'));await p.click('#offer');await p.waitForFunction(()=>document.querySelector('#offer-json').value.length>0);
 fs.writeFileSync(offer,await p.inputValue('#offer-json'));host=cp.spawn(bin,['remote-answer',config,invite,offer,answer],{stdio:['ignore','pipe','pipe']});let err='';host.stderr.on('data',b=>err+=b);
 for(let i=0;i<200&&!fs.existsSync(answer);i++){if(host.exitCode!==null)throw new Error('Host failed: '+err);await pause(100);}if(!fs.existsSync(answer))throw new Error('No answer '+err);
 const sockets=cp.execFileSync(process.env.LSOF_BIN||(process.platform==='darwin'?'/usr/sbin/lsof':'lsof'),['-nP','-a','-p',String(host.pid),'-iUDP','-Fn'],{encoding:'utf8'});for(const line of sockets.split('\n').filter(x=>x.startsWith('n')))if(!line.startsWith('n127.0.0.1:')&&!line.startsWith('n[::1]:'))throw new Error('Unexpected nonloopback listener');
 await p.fill('#answer-json',fs.readFileSync(answer,'utf8'));await p.click('#connect');await p.waitForFunction(()=>!document.querySelector('#login').disabled);
 await p.fill('#password','only-a-test-password-123');await p.click('#login');await p.waitForFunction(()=>document.querySelector('#tool').options.length>0);await p.selectOption('#tool',process.env.HARBOR_TEST_TOOL||'cat');await p.click('#start');await p.waitForFunction(()=>document.querySelector('#status').textContent.includes('running'));

 if(process.env.HARBOR_TEST_TOOL==='codex'){await p.waitForFunction(()=>document.querySelector('.xterm-rows').textContent.includes('Trust this folder'));fs.mkdirSync(path.join(root,'docs/screenshots'),{recursive:true});await p.screenshot({path:path.join(root,'docs/screenshots/codex-trust.png'),fullPage:true});console.log('Real Codex native folder-trust prompt rendered; no trust change or model request submitted.');}else{ await p.locator('.xterm-helper-textarea').pressSequentially('remote-proof-through-authenticated-channel');await p.locator('.xterm-helper-textarea').press('Enter');await p.waitForFunction(()=>document.querySelector('.xterm-rows').textContent.includes('remote-proof-through-authenticated-channel'));}

 await p.click('#stop');await p.waitForFunction(()=>document.querySelector('#status').textContent.includes('Stop requested'));await p.click('#logout');await p.waitForFunction(()=>document.querySelector('#status').textContent.includes('Logged out')||document.querySelector('#status').textContent.includes('closed'));
 for(let i=0;i<50&&host.exitCode===null;i++)await pause(100);if(host.exitCode===null)throw new Error('Host did not finish on close');if(errors.length)throw new Error(errors.join('\n'));
 const copied=path.join(run,'copied-invite.json');fs.copyFileSync(invite,copied);const reused=cp.spawnSync(bin,['remote-answer',config,copied,offer,path.join(run,'reused-answer.json')]);if(reused.status===0||fs.existsSync(path.join(run,'reused-answer.json')))throw new Error('Consumed grant reused');
 console.log('PASS: browser→Noise/WebRTC→Rust principal/password authentication→registered PTY input/output→stop/logout; native loopback sockets; consumed-grant rejection.');
 // A second fresh peer proves one config edit revokes separate running OS processes.
 const invite2=path.join(run,'revoke-invite.json'),offer2=path.join(run,'revoke-offer.json'),answer2=path.join(run,'revoke-answer.json');
 cp.execFileSync(bin,['remote-invite',config,'tester',invite2]);
 await p.reload();await p.waitForFunction(()=>document.querySelector('#status').textContent.startsWith('Ready.'));
 await p.setInputFiles('#invite',invite2);await p.waitForFunction(()=>document.querySelector('#identity').textContent.includes('tester'));
 await p.click('#offer');await p.waitForFunction(()=>document.querySelector('#offer-json').value.length>0);
 fs.writeFileSync(offer2,await p.inputValue('#offer-json'));
 host=cp.spawn(bin,['remote-answer',config,invite2,offer2,answer2],{stdio:['ignore','pipe','pipe']});
 for(let i=0;i<200&&!fs.existsSync(answer2);i++){if(host.exitCode!==null)throw new Error('Revocation peer exited early');await pause(100);}
 if(!fs.existsSync(answer2))throw new Error('Revocation peer answer missing');
 await p.fill('#answer-json',fs.readFileSync(answer2,'utf8'));await p.click('#connect');
 await p.waitForFunction(()=>!document.querySelector('#login').disabled);
 await p.fill('#password','only-a-test-password-123');await p.click('#login');
 await p.waitForFunction(()=>document.querySelector('#tool').options.length>0);
 await p.selectOption('#tool','cat');await p.click('#start');
 await p.waitForFunction(()=>document.querySelector('#status').textContent.includes('running'));
 const localToken=await p.evaluate(async()=>{const r=await fetch('/api/login',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({username:'tester',password:'only-a-test-password-123'})});if(!r.ok)throw new Error('Local revocation fixture login failed');return(await r.json()).token;});
 cp.execFileSync(bin,['remove-user',config,'tester']);
 const rejected=await p.evaluate(async token=>(await fetch('/api/terminal',{method:'POST',headers:{'Content-Type':'application/json',Authorization:`Bearer ${token}`},body:JSON.stringify({action:'read',sequence:0,offset:0})})).status,localToken);
 if(rejected!==503)throw new Error(`Local stale config accepted authority: ${rejected}`);
 await p.waitForFunction(()=>document.querySelector('#start').disabled&&document.querySelector('#tool').options.length===0,{},{timeout:10000});
 for(let i=0;i<100&&host.exitCode===null;i++)await pause(100);
 if(host.exitCode===null)throw new Error('Revoked peer failed to close');
 console.log('PASS: one host config edit revoked both HTTP gateway and independent WebRTC peer process.');

 }finally{if(browser)await browser.close();if(host&&host.exitCode===null){for(let i=0;i<30&&host.exitCode===null;i++)await pause(100);if(host.exitCode===null){host.kill('SIGINT');await new Promise(resolve=>host.once('exit',resolve));}}if(server&&server.exitCode===null){server.kill('SIGINT');await new Promise(resolve=>server.once('exit',resolve));}fs.rmSync(run,{recursive:true,force:true});}})().catch(e=>{console.error(e);process.exit(1)});
