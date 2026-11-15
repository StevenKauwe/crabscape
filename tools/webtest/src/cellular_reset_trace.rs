//! Test-only reset diagnostics. Keeps the existing reset deadline/assertions unchanged.
pub(super) const INSTALL: &str = r#"(() => {
 const events=[],ids=new WeakMap();let next=0;
 const id=o=>{if(!ids.has(o))ids.set(o,++next);return ids.get(o)};
 const record=(kind,details={})=>{events.push({at:performance.now(),kind,...details});if(events.length>400)events.shift();};
 window.__cellularResetTrace=events;
 const canvas=document.getElementById('crabscape');
 canvas.setAttribute('data-cell-work-profile','1');canvas.setAttribute('data-cell-performance-probe','1');
 const opening=IDBFactory.prototype.open,transaction=IDBDatabase.prototype.transaction,close=IDBDatabase.prototype.close;
 const get=IDBObjectStore.prototype.get,clear=IDBObjectStore.prototype.clear,put=IDBObjectStore.prototype.put,abort=IDBTransaction.prototype.abort;
 IDBFactory.prototype.open=function(...args){record('open',{name:args[0]});const r=opening.apply(this,args);
  for(const event of ['success','error','blocked','upgradeneeded'])r.addEventListener(event,()=>{record('open-'+event,{name:args[0]});if(event==='success')r.result.addEventListener('versionchange',()=>record('versionchange',{db:id(r.result)}));});return r;};
 IDBDatabase.prototype.transaction=function(...args){const tx=transaction.apply(this,args);record('transaction',{db:id(this),tx:id(tx),mode:tx.mode});
  for(const event of ['complete','abort','error'])tx.addEventListener(event,()=>record('transaction-'+event,{tx:id(tx)}));return tx;};
 IDBDatabase.prototype.close=function(){record('close',{db:id(this)});return close.call(this)};
 IDBTransaction.prototype.abort=function(){record('abort-called',{tx:id(this)});return abort.call(this)};
 IDBObjectStore.prototype.get=function(key){const r=get.call(this,key);if(typeof key==='string'){const tx=id(this.transaction);record('get',{tx,key});
  for(const event of ['success','error'])r.addEventListener(event,()=>record('get-'+event,{tx,key}));}return r;};
 IDBObjectStore.prototype.clear=function(){const tx=id(this.transaction);record('clear',{tx});const r=clear.call(this);
  for(const event of ['success','error'])r.addEventListener(event,()=>record('clear-'+event,{tx}));return r;};
 IDBObjectStore.prototype.put=function(...args){if(typeof args[1]==='string')record('put',{tx:id(this.transaction),key:args[1]});return put.apply(this,args)};
 const observer=new PerformanceObserver(list=>{for(const e of list.getEntries())if(e.name.startsWith('cell-page-')||e.name.startsWith('cell-save-'))record(e.name,{duration:e.duration});});
 observer.observe({type:'measure',buffered:false});
 setInterval(()=>record('state',{page:canvas.getAttribute('data-cell-page-state'),save:canvas.getAttribute('data-cell-save'),message:document.getElementById('message')?.textContent}),1000);
 record('installed');return true;
})()"#;
