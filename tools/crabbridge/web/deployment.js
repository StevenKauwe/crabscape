// Configuration guard, not proof of publisher/origin integrity. A compromised origin can replace this code.
export const PRODUCTION_CSP="default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; connect-src 'self' wss://crabscape-introducer.fly.dev; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";
export const DEVELOPMENT_CSP=PRODUCTION_CSP.replace("connect-src 'self'", "connect-src 'self' http://127.0.0.1:7879 ws://127.0.0.1:3536");
export function validateDeploymentHeaders(headers,development=false){
 const expected=new Map((development?DEVELOPMENT_CSP:PRODUCTION_CSP).split(';').map(x=>x.trim().split(/\s+/)).map(([key,...v])=>[key,v.sort().join(' ')]));
 const actual=new Map();for(const raw of (headers.get('content-security-policy')||'').split(';')){const [key,...values]=raw.trim().split(/\s+/);if(!key)continue;if(actual.has(key.toLowerCase()))throw new Error('Duplicate deployment CSP');actual.set(key.toLowerCase(),values.sort().join(' '));}
 if(actual.size!==expected.size)throw new Error('Unexpected deployment CSP directive');
 for(const [key,value]of expected)if(actual.get(key)!==value)throw new Error('Required deployment CSP missing or different');
 if(headers.get('x-frame-options')?.toUpperCase()!=='DENY'||headers.get('referrer-policy')!=='no-referrer'||headers.get('x-content-type-options')!=='nosniff'||headers.get('x-crabbridge-policy')!=='1'||headers.get('cache-control')!=='no-store')throw new Error('Required deployment headers missing');
 const permissions=(headers.get('permissions-policy')||'').split(',').map(x=>x.trim()).sort();if(permissions.join(',')!=='camera=(),geolocation=(),microphone=()')throw new Error('Required permission policy missing or ambiguous');
}
export async function checkDeployment(){
 if(window.top!==window.self)throw new Error('Open CrabBridge directly, not in a frame.');
 if(navigator.serviceWorker?.controller)throw new Error('A service worker controls this page. Use an approved clean origin.');
 const url=new URL(location.href),development=url.protocol==='http:'&&url.hostname==='127.0.0.1';
 if(!development&&(url.protocol!=='https:'||!window.isSecureContext))throw new Error('Trusted HTTPS hosting is required.');
 url.hash='';const response=await fetch(url,{method:'HEAD',credentials:'omit',redirect:'error',cache:'no-store',referrerPolicy:'no-referrer',signal:AbortSignal.timeout(5000)});
 if(!response.ok||new URL(response.url).origin!==url.origin)throw new Error('Deployment policy response failed');validateDeploymentHeaders(response.headers,development);if(navigator.serviceWorker?.controller)throw new Error('Service worker took control during policy check');return{development};
}
