// Small browser bridge to the Rust Matchbox transport. No save documents leave this device.
const peerListeners = new Set(), connectionListeners = new Set(), topics = new Map();
const peers = new Map();
let socket = null, id = null, connected = false, wanted = null, snapshot = null;
let importing = null, reopening = false, nextConnect = 0, lastHello = 0, attempt = 0;
let line = 'Fly solo, or invite friends to a private P2P party. Your progress stays on this device.';
const params = new URLSearchParams(location.search);
const override = params.get('introducer');
const loopback = ['127.0.0.1', 'localhost'].includes(location.hostname);
const introducer = loopback && /^ws:\/\/127\.0\.0\.1:\d+$/.test(override || '')
  ? override : 'wss://crabscape-introducer.fly.dev';
// Preview and live copies cannot accidentally meet in the same room.
const build = location.pathname.replace(/\/dragon\/?(?:index\.html)?$/, '').replace(/[^a-zA-Z0-9-]/g, '-').slice(-32) || 'live';
const clone = v => JSON.parse(JSON.stringify(v));
const rider = v => Number.isInteger(v) && v >= 0 && v < 6;
const notifyPeers = () => {
  const all = [...peers].filter(([, p]) => p.presence).map(([peer, p]) => ({ peer, presence: clone(p.presence), kind: 'viewer', sameTab: false }));
  if (id && snapshot) all.push({ peer: id, presence: clone(snapshot), kind: 'viewer', sameTab: true });
  for (const fn of peerListeners) fn({ peers: all });
};
function status(text) {
  line = text;
  for (const el of document.querySelectorAll('[data-p2p-status]')) el.textContent = text;
  document.documentElement.dataset.dragonLink = connected ? 'connected' : wanted ? 'connecting' : 'solo';
}
function connection(value) {
  if (connected === value) return;
  connected = value;
  for (const fn of connectionListeners) fn(value);
}
function send(topic, data, reliable = true) {
  return !!socket && socket.send(topic, JSON.stringify(data), reliable);
}
// A packet's claimed rider must belong to its real data-channel sender. Collisions are settled by peer id.
function accepted(peer, r) {
  if (!rider(r) || peers.get(peer)?.presence?.r !== r) return false;
  if (snapshot?.r === r && id && id < peer) return false;
  return ![...peers].some(([other, p]) => other < peer && p.presence?.r === r);
}
function receive(event) {
  const peer = peers.get(event.peer);
  if (!peer) return;
  const data = event.data;
  if (event.topic === 'hello') {
    peer.compatible = true;
    if (snapshot) send('presence', snapshot);
    return;
  }
  // All packets are also exact-version-checked in Rust.
  if (event.topic === 'presence') {
    if (!data || !rider(data.r) || !Number.isFinite(data.x) || !Number.isFinite(data.y)) return;
    peer.presence = data;
    peer.seen = performance.now();
    notifyPeers();
    return;
  }
  if (!accepted(event.peer, data?.r)) return;
  for (const fn of topics.get(event.topic) || []) fn({ data: clone(data), peer: event.peer, sameTab: false });
}
async function open() {
  if (!wanted || reopening) return;
  reopening = true;
  const token = ++attempt, code = wanted;
  try {
    importing ||= (async () => {
      const version = await fetch('./pkg/version.txt', { cache: 'no-store' }).then(r => { if (!r.ok) throw Error('P2P module unavailable'); return r.text(); });
      if (!/^\d+\s*$/.test(version)) throw Error('P2P module unavailable');
      const module = await import(`./pkg/dragon_peer.js?v=${version.trim()}`);
      await module.default({ module_or_path: new URL(`./pkg/dragon_peer_bg.wasm?v=${version.trim()}`, location.href) });
      return module;
    })();
    const { DragonPeer } = await importing;
    if (token !== attempt || wanted !== code) return;
    socket?.free();
    socket = new DragonPeer(introducer, build, wanted);
    id = null; peers.clear(); notifyPeers();
    nextConnect = performance.now() + 15000;
    status('Connecting your private P2P party. You can keep playing solo.');
  } catch (error) {
    if (token !== attempt) return;
    status('P2P is unavailable right now. Your solo game and saved Rider still work.');
    importing = null; nextConnect = performance.now() + 15000;
  } finally { if (token === attempt) reopening = false; }
}
const room = {
  presence(data) {
    snapshot = clone(data);
    send('presence', snapshot, false);
    return Promise.resolve();
  },
  emit(topic, data) { send(topic, data, topic !== 'world'); return Promise.resolve(); },
  on(topic, fn) { if (!topics.has(topic)) topics.set(topic, new Set()); topics.get(topic).add(fn); return () => topics.get(topic).delete(fn); },
  onPeers(fn) { peerListeners.add(fn); queueMicrotask(notifyPeers); return () => peerListeners.delete(fn); },
  onConnection(fn) { connectionListeners.add(fn); queueMicrotask(() => fn(connected)); return () => connectionListeners.delete(fn); },
  connected: () => connected,
  peerId: () => id,
  status: () => line,
  leave() {
    wanted = null; attempt++; reopening = false; socket?.free(); socket = null; id = null; peers.clear();
    notifyPeers(); connection(false);
    const url = new URL(location.href); url.searchParams.delete('join'); history.replaceState(null, '', url);
    status('Flying solo. Your progress stays on this device.');
  },
};
window.valmoraP2PResolve(room);
async function invite(event) {
  const button = event?.currentTarget;
  if (wanted && !socket && !reopening) { nextConnect = 0; open(); }
  if (!wanted) {
    try { wanted = localStorage.getItem('vdf-p2p-lobby'); } catch (_) {}
    if (!/^[a-z0-9]{6,16}$/.test(wanted || '')) {
      wanted = [...crypto.getRandomValues(new Uint8Array(10))].map(n => (n % 36).toString(36)).join('');
      try { localStorage.setItem('vdf-p2p-lobby', wanted); } catch (_) {}
    }
    open();
  }
  const url = new URL(location.href); url.searchParams.set('join', wanted);
  // Do not share local test infrastructure in public invite links.
  if (!loopback) url.searchParams.delete('introducer');
  try {
    await navigator.clipboard.writeText(url.href);
    status('Invite link copied. Friends who open it can join your Party. Progress stays on each device.');
  } catch (_) {
    const box = document.getElementById('p2pLink'); button?.after(box); box.value = url.href; box.hidden = false; box.focus(); box.select();
    status('Copy the invite link below and send it to your friends.');
  }
}
for (const button of document.querySelectorAll('[data-p2p-invite]')) button.addEventListener('click', invite);
status(line);
const joined = params.get('join');
if (/^[a-z0-9]{6,16}$/.test(joined || '')) { wanted = joined; open(); }
setInterval(() => {
  const now = performance.now();
  if (!socket) { if (wanted && now >= nextConnect) open(); return; }
  try {
    const state = JSON.parse(socket.poll());
    id = state.id;
    if (state.ended || (!id && now >= nextConnect)) {
      socket.free(); socket = null; peers.clear(); notifyPeers(); connection(false);
      nextConnect = now + 5000; status('Party connection interrupted. Reconnecting; your solo progress is safe.'); return;
    }
    if (id) connection(true);
    for (const event of state.events) {
      if (event.type === 'joined') { peers.set(event.peer, { presence: null, seen: now }); send('hello', {}); if (snapshot) send('presence', snapshot); }
      else if (event.type === 'left') { peers.delete(event.peer); notifyPeers(); }
      else receive(event);
    }
    if (now - lastHello > 1000) {
      lastHello = now; send('hello', {}); if (snapshot) send('presence', snapshot);
      for (const [peer, data] of peers) if (data.presence && now - data.seen > 5000) { data.presence = null; notifyPeers(); }
      const count = [...peers.values()].filter(p => p.presence).length;
      if (connected) status(count ? `P2P party connected with ${count} friend${count === 1 ? '' : 's'}. Progress stays on each device.` : 'Your P2P party is open. Waiting for friends with this invite link.');
    }
  } catch (_) {
    socket?.free(); socket = null; peers.clear(); notifyPeers(); connection(false); nextConnect = now + 5000;
    status('Party connection interrupted. Reconnecting; you can keep playing solo.');
  }
}, 40);
