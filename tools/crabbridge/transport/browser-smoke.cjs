const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const fs=require('fs'),path=require('path'),cp=require('child_process');
const root=__dirname,run=fs.mkdtempSync('/tmp/harbor-noise-'),bin=path.join(root,'target/debug/harbor-transport-spike');
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
(async()=>{const server=cp.spawn('/usr/bin/python3',['-m','http.server','8787','--bind','127.0.0.1'],{cwd:path.join(root,'web'),stdio:'ignore'});let browser;let host;
try {cp.execFileSync(bin,['keygen',path.join(run,'key')],{stdio:'ignore'});const invitation=JSON.parse(fs.readFileSync(path.join(run,'key'),'utf8'));const key=invitation.key;await sleep(500);browser=await chromium.launch({headless:true,executablePath:'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'});
for(const wrong of [false,true]) {const page=await browser.newPage();page.on('pageerror',e=>console.error('Page error:',String(e)));await page.goto('http://127.0.0.1:8787');await page.waitForFunction(()=>document.querySelector('#status').textContent.startsWith('Ready.'));
await page.fill('#key',wrong?'02'.repeat(32):key);await page.fill('#context',JSON.stringify(invitation.context));await page.click('#offer');await page.waitForFunction(()=>document.querySelector('#offer-json').value.length>0);
const offer=path.join(run,wrong?'bad-offer.json':'offer.json'),answer=path.join(run,wrong?'bad-answer.json':'answer.json');fs.writeFileSync(offer,await page.inputValue('#offer-json'));
host=cp.spawn(bin,['answer',path.join(run,'key'),offer,answer],{stdio:['ignore','pipe','pipe']});let errors='';host.stderr.on('data',b=>errors+=b);for(let i=0;i<200&&!fs.existsSync(answer);i++){if(host.exitCode!==null)throw new Error('Host exited: '+errors);await sleep(100);}if(!fs.existsSync(answer))throw new Error('No answer: '+errors);
const sockets=cp.execFileSync('/usr/sbin/lsof',['-nP','-a','-p',String(host.pid),'-iUDP','-Fn'],{encoding:'utf8'});for(const line of sockets.split('\n').filter(x=>x.startsWith('n'))){if(!line.startsWith('n127.0.0.1:')&&!line.startsWith('n[::1]:'))throw new Error('Nonloopback native socket: '+line);}console.log('Native UDP sockets verified loopback-only');await page.fill('#answer-json',fs.readFileSync(answer,'utf8'));await page.click('#connect');
if(wrong){await page.waitForFunction(()=>document.querySelector('#status').textContent.includes('Channel closed'),{timeout:30000});if(!await page.locator('#send').isDisabled())throw new Error('Wrong key enabled send');console.log('Wrong-key WebRTC peer denied');}
else{await page.waitForFunction(()=>!document.querySelector('#send').disabled,{timeout:30000});await page.click('#send');await page.waitForFunction(()=>document.querySelector('#status').textContent.includes('authenticated browser to Rust'),{timeout:30000});console.log('Browser WASM → WebRTC → native Rust authenticated encrypted echo passed');}
await page.close();if(host.exitCode===null){host.kill('SIGINT');await new Promise(resolve=>host.once('exit',resolve));}host=null;
}
}finally{if(host)host.kill('SIGINT');if(browser)await browser.close();server.kill();fs.rmSync(run,{recursive:true,force:true});}})().catch(e=>{console.error(e);process.exit(1)});
