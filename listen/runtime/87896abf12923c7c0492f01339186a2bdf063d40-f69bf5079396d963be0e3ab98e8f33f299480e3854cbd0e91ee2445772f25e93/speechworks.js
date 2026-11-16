import {Transcript} from './transcript.js';
import {createPoolOwner} from './pool-owner.js';
import {hasCachedModel} from './model-cache.js';

/** Local dictation. Models, backend and phrase policy are application config.
 * Audio is sent only to a local Web Worker. No network transcription fallback. */
export function createDictation(options = {}) {
  const workerUrl = options.workerUrl || new URL('./worker-entry.js', import.meta.url);
  const manifestUrl = new URL(options.manifestUrl || './models.json', import.meta.url).href;
  const listeners = new Set(), requests = new Map();
  const pools = new WeakMap();
  let worker, configuring, loaded, active = null, sequence = 0;
  let status = {ready: false, progress: 0};
  const cancelledError = () => new DOMException('Dictation cancelled.', 'AbortError');

  function update(change) {
    status = {...status, ...change};
    for (const fn of listeners) {
      // Subscriber failures cannot prevent settling requests or releasing capture.
      try { fn({...status}); } catch { /* The application owns its status UI. */ }
    }
  }
  function rejectAll(error, connection = worker) {
    if (connection !== worker) return;
    worker = null; configuring = null; loaded = null;
    // Children are owned here, not by a possibly blocked inference coordinator.
    if (connection) { pools.get(connection)?.dispose(); pools.delete(connection); }
    connection?.terminate();
    for (const pending of requests.values()) pending.reject(error);
    requests.clear();
    active?.fail(error);
    update({ready: false, error: error.message});
  }
  function rejectSession(id, error) {
    for (const [key, pending] of requests) if (pending.session === id) {
      requests.delete(key); pending.reject(error);
    }
  }
  function requireConnection(connection) {
    if (!connection || connection !== worker) throw new Error('Speech engine closed or replaced.');
  }
  function request(type, data = {}, connection = worker) {
    const id = ++sequence;
    return new Promise((resolve, reject) => {
      requests.set(id, {resolve, reject, session: data.session});
      try {
        requireConnection(connection);
        connection.postMessage({type, id, ...data});
      } catch (error) {
        requests.delete(id); reject(error);
      }
    });
  }
  function boot() {
    if (configuring) return configuring;
    const connection = new Worker(workerUrl, {type: 'module'});
    worker = connection;
    pools.set(connection, createPoolOwner({
      workerUrl: new URL(workerUrl, globalThis.location?.href || import.meta.url),
      postMessage: message => connection.postMessage(message),
      onFatal: error => rejectAll(error, connection),
    }));
    connection.onerror = event => {
      event.preventDefault();
      rejectAll(new Error(event.message || 'Speech worker stopped.'), connection);
    };
    connection.onmessageerror = () => rejectAll(new Error('Speech worker returned an unreadable message.'), connection);
    connection.onmessage = ({data: m}) => {
      if (worker !== connection) return;
      if (pools.get(connection)?.handleMessage(m)) return;
      if (m.type === 'progress') update({progress: m.value, model: m.model});
      if (m.type === 'backend') update({device:m.device, fallback:m.fallback, model:m.model});
      if (m.type === 'error') {
        const error = new Error(m.message || 'Speech recognition failed.');
        if (m.id && requests.has(m.id)) { requests.get(m.id).reject(error); requests.delete(m.id); }
        else if (m.session && active?.id === m.session) active.fail(error);
        else if (!m.id && !m.session) rejectAll(error, connection);
      } else if (m.id && requests.has(m.id)) { requests.get(m.id).resolve(m); requests.delete(m.id); }
      if (m.session && active?.id === m.session) active.receive(m);
    };
    configuring = request('configure', {options: {
      manifestUrl, poolOwner: 'ui-v1', cpuThreads: options.cpuThreads, backend: options.backend || 'auto', models: options.models, session: options.session,
    }}).catch(error => { rejectAll(error, connection); throw error; });
    return configuring;
  }
  async function preload() {
    if (loaded) return loaded;
    const ready = boot(), connection = worker;
    const pending = (async () => {
      await ready;
      await request('preload', {}, connection);
      requireConnection(connection);
      update({ready: true, progress: 1, error: undefined});
    })().catch(error => {
      if (worker === connection && loaded === pending) {
        loaded = null;
        update({ready: false, error: error.message});
      }
      throw error;
    });
    loaded = pending;
    return pending;
  }
  async function isCached() {
    try {
      if(new URL(manifestUrl).origin !== globalThis.location.origin) return false;
      const response = await fetch(manifestUrl,{cache:'no-store',redirect:'error'}); if (!response.ok) return false;
      const manifest = await response.json(); const cache = await caches.open('speechworks-models-v1');
      const profile = {...manifest.defaults, ...options.models};
      const ids = options.session?.previews === false ? [profile.final] : [profile.final, profile.draft];
      return (await Promise.all(ids.map(id => hasCachedModel(cache,new URL(manifest.models[id].url,manifestUrl).href,manifest.models[id])))).every(Boolean);
    } catch { return false; }
  }
  async function record({onText = () => {}, onError = () => {}, signal, session = {}} = {}) {
    if (active) throw new Error('Dictation is already active.');
    const id = ++sequence, transcript = new Transcript();
    let connection, stream, context, tap, analyser, finished = false, captureClosed = false;
    let stopPromise, flushWaiter, outstanding = 0, failure;
    let resolveDone, rejectDone, rejectCancelled;
    const done = new Promise((resolve, reject) => { resolveDone = resolve; rejectDone = reject; });
    const cancelled = new Promise((_, reject) => { rejectCancelled = reject; });
    done.catch(() => {}); cancelled.catch(() => {});
    const wait = async operation => {
      const value = await Promise.race([operation, cancelled]);
      if (finished) throw failure || cancelledError();
      requireConnection(connection);
      return value;
    };
    const closeCapture = error => {
      if (flushWaiter) { flushWaiter.reject(error || cancelledError()); flushWaiter = null; }
      if (captureClosed) return;
      captureClosed = true;
      if (tap) tap.port.onmessage = null;
      stream?.getTracks().forEach(track => track.stop());
      context?.close().catch(() => {});
    };
    const close = error => {
      closeCapture(error);
      signal?.removeEventListener('abort', abort);
    };
    const fail = error => {
      if (finished) return;
      failure = error; finished = true; close(error);
      // Closing a failed worker or a detached port must not interrupt cleanup.
      try { if (connection === worker) connection?.postMessage({type: 'cancel', session: id}); } catch {}
      if (active === state) active = null;
      rejectSession(id, error); rejectCancelled(error); rejectDone(error);
      try { onError(error); } catch { /* A failing error callback must not keep the mic open. */ }
    };
    const abort = () => fail(cancelledError());
    const state = {id, fail, receive(m) {
      if (finished) return;
      try {
        if (m.type === 'audio-ack') outstanding = Math.max(0, outstanding - m.samples);
        if (m.type === 'live' || m.type === 'final') onText(transcript.update(m));
        if (m.type === 'done') {
          const text = transcript.finish();
          close();
          // Leave fail() available until the callback returns: a UI exception
          // must reject stop(), rather than mark finished and strand its promise.
          onText({committed: text, live: ''});
          if (finished) return;
          finished = true; if (active === state) active = null; resolveDone(text);
        }
      } catch (error) { fail(error); }
    }};
    active = state;
    signal?.addEventListener('abort', abort, {once: true});
    try {
      if (signal?.aborted) throw cancelledError();
      const ready = preload(); connection = worker;
      await wait(ready);
      const capture = navigator.mediaDevices.getUserMedia({audio: {
        channelCount: 1, echoCancellation: true, noiseSuppression: true, autoGainControl: true,
      }}).then(value => {
        // Permission can resolve after cancellation. That late stream still
        // belongs to this recording and must be released immediately.
        if (finished) { value.getTracks().forEach(track => track.stop()); throw failure || cancelledError(); }
        stream = value; return value;
      });
      await wait(capture);
      let source;
      try { context = new AudioContext({sampleRate: 16000}); source = context.createMediaStreamSource(stream); }
      catch {
        await wait(context?.close()); context = new AudioContext(); source = context.createMediaStreamSource(stream);
      }
      await wait(context.audioWorklet.addModule(new URL('./capture-worklet.js', import.meta.url)));
      tap = new AudioWorkletNode(context, 'speechworks-capture');
      analyser = context.createAnalyser(); analyser.fftSize = 512;
      await wait(request('start', {session: id, config: session}, connection));
      tap.port.onmessage = ({data}) => {
        if (data.flushed) { const waiter = flushWaiter; flushWaiter = null; waiter?.resolve(); return; }
        if (finished || !data.samples) return;
        try {
          outstanding += data.samples.length;
          if (outstanding > context.sampleRate * 5) throw new Error('Speech recognition cannot keep up with this microphone. Try final-only mode.');
          requireConnection(connection);
          connection.postMessage({type: 'audio', session: id, samples: data.samples, sampleRate: context.sampleRate}, [data.samples.buffer]);
        } catch (error) { fail(error); }
      };
      source.connect(analyser); source.connect(tap); tap.connect(context.destination);
      await wait(context.resume());
    } catch (error) { fail(error); close(error); if (active === state) active = null; throw error; }
    return {
      cancel() { abort(); },
      level() {
        if (finished) return 0;
        const values = new Float32Array(analyser.fftSize); analyser.getFloatTimeDomainData(values);
        return Math.min(1, Math.sqrt(values.reduce((sum, x) => sum + x * x, 0) / values.length) * 4);
      },
      stop() {
        if (stopPromise) return stopPromise;
        stopPromise = (async () => {
          if (finished) return done;
          let timer;
          try {
            // Small trailing capture helps preserve the final consonant.
            try { await wait(new Promise(resolve => { timer = setTimeout(resolve, options.stopTailMs ?? 200); })); }
            finally { clearTimeout(timer); }
            try {
              await wait(new Promise((resolve, reject) => {
                flushWaiter = {resolve, reject};
                timer = setTimeout(() => reject(new Error('Microphone did not flush.')), 2000);
                tap.port.postMessage('flush');
              }));
            } finally { clearTimeout(timer); flushWaiter = null; }
            closeCapture();
            requireConnection(connection);
            connection.postMessage({type: 'stop', session: id});
          } catch (error) { fail(error); }
          return done;
        })();
        return stopPromise;
      },
    };
  }
  return {
    onStatus(fn) { listeners.add(fn); try { fn({...status}); } catch {} return () => listeners.delete(fn); },
    isCached, preload, record,
    async transcribe(samples, {model, priority = false} = {}) {
      const ready = boot(), connection = worker;
      await ready;
      const result = await request('transcribe', {samples, model, priority}, connection);
      requireConnection(connection);
      return result.text;
    },
    destroy() {
      active?.fail(new DOMException('Speech engine closed.', 'AbortError'));
      rejectAll(new Error('Speech engine closed.')); listeners.clear();
    },
  };
}
