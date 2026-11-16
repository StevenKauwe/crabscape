// The UI owns CPU Worker handles, so it can stop them even when the inference
// coordinator is blocked inside synchronous WASM. No shared/descendant-worker
// lifetime assumptions are needed. Timeouts are failure guards, not latency goals.
export const CPU_STARTUP_TIMEOUT_MS = 30000;
export const CPU_INFERENCE_TIMEOUT_MS = 30000;
const describe = error => error?.message || String(error);
const prefix = 'speechworks:pool-';

/** Coordinator-side adapter consumed by the optional Rayon helper. */
export function createWorkerOwner(postMessage) {
  let sequence = 0;
  const pending = new Map();
  const errorListeners = new Set();
  const call = (operation, payload) => new Promise((resolve, reject) => {
    const id = ++sequence;
    pending.set(id, {resolve, reject});
    try { postMessage({type: prefix + operation, id, payload}); }
    catch (error) { pending.delete(id); reject(error); }
  });
  return {
    spawn: payload => call('spawn', payload),
    dispose: () => call('dispose'),
    onError(fn) { errorListeners.add(fn); return () => errorListeners.delete(fn); },
    handleMessage(data) {
      if (data?.type === prefix + 'result') {
        const request = pending.get(data.id);
        if (request) {
          pending.delete(data.id);
          data.ok ? request.resolve() : request.reject(new Error(data.message || 'CPU worker pool failed.'));
        }
        return true;
      }
      if (data?.type === prefix + 'fatal') {
        const error = new Error(data.message || 'CPU worker failed.');
        for (const listener of errorListeners) listener(error);
        return true;
      }
      return false;
    },
  };
}

/** UI-side pool owner. It handles only messages from its associated coordinator. */
export function createPoolOwner({
  workerUrl,
  postMessage,
  onFatal,
  createWorker = (url, options) => new Worker(url, options),
  startupTimeoutMs = CPU_STARTUP_TIMEOUT_MS,
  inferenceTimeoutMs = CPU_INFERENCE_TIMEOUT_MS,
}) {
  const base = new URL('./', workerUrl);
  const workers = new Set();
  const cancellations = new Set();
  let started = false;
  let stopped = false;
  let poolReady = false;
  let watchdog;
  let activeToken;
  function disposePool() {
    clearTimeout(watchdog); watchdog = undefined; activeToken = undefined;
    for (const cancel of [...cancellations]) cancel(new Error('CPU worker pool terminated.'));
    for (const record of workers) {
      record.worker.removeEventListener('message', record.onMessage);
      record.worker.removeEventListener('error', record.onError);
      record.worker.removeEventListener('messageerror', record.onError);
      record.worker.terminate();
    }
    workers.clear();
  }
  function fatal(error) {
    if (stopped) return;
    stopped = true;
    disposePool();
    onFatal(error instanceof Error ? error : new Error(describe(error)));
  }
  function validate({url, count, init} = {}) {
    const worker = new URL(url);
    const main = new URL(init?.mainJS);
    const directory = new URL('threaded/', base);
    if (worker.origin !== base.origin || !worker.href.startsWith(directory.href)
        || main.href !== new URL('speechworks_web.js', directory).href) {
      throw new Error('CPU worker and module must belong to this same-origin runtime.');
    }
    if (!Number.isSafeInteger(count) || count < 1 || count > 32
        || init?.type !== 'speechworks_rayon_init') throw new Error('Invalid CPU worker pool request.');
    return worker;
  }
  async function spawn(payload) {
    if (stopped || started) throw new Error('CPU worker pool owner cannot be reused.');
    const url = validate(payload);
    started = true;
    try {
      const pending = [];
      let rejectStartup;
      const startupFailure = new Promise((_, reject) => { rejectStartup = reject; });
      startupFailure.catch(() => {});
      for (let i = 0; i < payload.count; i++) {
        const worker = createWorker(url, {type: 'module', name: 'speechworks_wasm_rayon'});
        let resolveReady, rejectReady;
        const ready = new Promise((resolve, reject) => { resolveReady = resolve; rejectReady = reject; });
        ready.catch(() => {});
        let settled = false;
        const settle = error => {
          if (settled) return;
          settled = true; clearTimeout(timer); cancellations.delete(cancel);
          error ? rejectReady(error) : resolveReady();
        };
        const cancel = error => settle(error);
        const record = {worker};
        record.onError = event => {
          event.preventDefault?.();
          const error = new Error(event.message || 'CPU worker failed.');
          if (poolReady) fatal(error);
          else { settle(error); rejectStartup(error); }
        };
        record.onMessage = ({data}) => {
          if (data?.type === 'speechworks_rayon_ready') settle();
          else if (data?.type === 'speechworks_rayon_error') record.onError({message: data.message});
        };
        const timer = setTimeout(() => settle(new Error('CPU worker startup timed out.')), startupTimeoutMs);
        cancellations.add(cancel); workers.add(record);
        worker.addEventListener('message', record.onMessage);
        worker.addEventListener('error', record.onError);
        worker.addEventListener('messageerror', record.onError);
        pending.push(ready);
        worker.postMessage(payload.init);
      }
      await Promise.race([Promise.all(pending), startupFailure]);
      if (stopped) throw new Error('CPU worker pool owner stopped.');
      poolReady = true;
    } catch (error) { disposePool(); throw error; }
  }
  async function answer(data) {
    try {
      if (data.type === prefix + 'spawn') await spawn(data.payload);
      else disposePool();
      if (!stopped) postMessage({type: prefix + 'result', id: data.id, ok: true});
    } catch (error) {
      if (!stopped) {
        try { postMessage({type: prefix + 'result', id: data.id, ok: false, message: describe(error)}); }
        catch (deliveryError) { fatal(deliveryError); }
      }
    }
  }
  return {
    handleMessage(data) {
      if (data?.type === prefix + 'spawn' || data?.type === prefix + 'dispose') {
        void answer(data); return true;
      }
      if (data?.type === 'speechworks:inference-start') {
        if (workers.size && !stopped) {
          clearTimeout(watchdog);
          activeToken = data.token;
          watchdog = setTimeout(() => fatal(new Error(`Threaded CPU operation exceeded ${inferenceTimeoutMs} ms; engine stopped.`)), inferenceTimeoutMs);
        }
        return true;
      }
      if (data?.type === 'speechworks:inference-end') {
        if (data.token === activeToken) { clearTimeout(watchdog); watchdog = undefined; activeToken = undefined; }
        return true;
      }
      return false;
    },
    dispose() { stopped = true; disposePool(); },
    get size() { return workers.size; },
  };
}
