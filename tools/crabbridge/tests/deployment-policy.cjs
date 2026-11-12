const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
(async()=>{
 const source=fs.readFileSync(path.join(__dirname,'../web/deployment.js'),'utf8');const {PRODUCTION_CSP,DEVELOPMENT_CSP,validateDeploymentHeaders}=await import('data:text/javascript;base64,'+Buffer.from(source).toString('base64'));
 const base={'content-security-policy':PRODUCTION_CSP,'x-frame-options':'DENY','referrer-policy':'no-referrer','x-content-type-options':'nosniff','x-crabbridge-policy':'1','cache-control':'no-store','permissions-policy':'camera=(), microphone=(), geolocation=()'};
 assert.doesNotThrow(()=>validateDeploymentHeaders(new Headers(base)));
 for(const key of Object.keys(base)){const bad={...base};delete bad[key];assert.throws(()=>validateDeploymentHeaders(new Headers(bad)));}
 for(const csp of [PRODUCTION_CSP.replace("script-src 'self'", "script-src *"),PRODUCTION_CSP+'; script-src *',PRODUCTION_CSP.replace("frame-ancestors 'none'","frame-ancestors 'self'"),DEVELOPMENT_CSP,PRODUCTION_CSP+'; script-src-elem *',PRODUCTION_CSP+'; SCRIPT-SRC *'])assert.throws(()=>validateDeploymentHeaders(new Headers({...base,'content-security-policy':csp})));
 for(const policy of ['camera=(), microphone=(), geolocation=(), camera=*','camera=*, camera=(), microphone=(), geolocation=()','camera=(), microphone=(), geolocation=(), usb=*'])assert.throws(()=>validateDeploymentHeaders(new Headers({...base,'permissions-policy':policy})));
 assert.doesNotThrow(()=>validateDeploymentHeaders(new Headers({...base,'content-security-policy':DEVELOPMENT_CSP}),true));
 console.log('PASS: canonical production hosting policy rejects missing/loose/duplicate headers and development-only destinations.');
})().catch(e=>{console.error(e);process.exit(1)});
