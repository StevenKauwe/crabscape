"""Against local running server with disposable tester/cat registration only."""
import json, urllib.request, urllib.error, time
BASE='http://127.0.0.1:7878'
token=None
checks=0
def call(path,data):
 global checks
 headers={'Origin':BASE,'Content-Type':'application/json'}
 if token: headers['Authorization']='Bearer '+token
 with urllib.request.urlopen(urllib.request.Request(BASE+path,data=json.dumps(data).encode(),headers=headers)) as r:
  checks+=1
  assert r.headers['Cache-Control']=='no-store'
  return json.load(r) if r.status!=204 else {}
a=call('/api/login',{'username':'tester','password':'only-a-test-password-123'});token=a['token']
r=call('/api/terminal',{'action':'start','tool':'cat','sequence':0});assert r['result']['started']
r=call('/api/terminal',{'action':'input','sequence':1,'text':'literal text; not a shell command\n'});assert r['result']['accepted']
time.sleep(.1)
r=call('/api/terminal',{'action':'read','sequence':2,'offset':0});assert 'literal text; not a shell command' in r['result']['text']
try:call('/api/terminal',{'action':'input','sequence':2,'text':'replay'})
except urllib.error.HTTPError as e:assert e.code==409;checks+=1
else:raise AssertionError('replay accepted')
call('/api/terminal',{'action':'stop','sequence':3})
call('/api/logout',{})
try:call('/api/terminal',{'action':'read','sequence':4,'offset':0})
except urllib.error.HTTPError as e:assert e.code==401;checks+=1
else:raise AssertionError('logout not revoked')
print(f'{checks} live HTTP/PTY checks passed')
