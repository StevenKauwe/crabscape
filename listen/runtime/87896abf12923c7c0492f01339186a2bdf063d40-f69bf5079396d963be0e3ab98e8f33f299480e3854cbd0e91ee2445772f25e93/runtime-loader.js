import {runtimeConfig} from './runtime-config.js';

const message = error => error?.message || String(error);

// A minimal () -> () module containing f32x4.relaxed_madd, then drop.
const zero = [0xfd, 0x0c, ...new Array(16).fill(0)];
const body = [0x00, ...zero, ...zero, ...zero, 0xfd, 0x85, 0x02, 0x1a, 0x0b];
const relaxedProbe = new Uint8Array([
  0x00, 0x61, 0x73, 0x6d, 1, 0, 0, 0,
  1, 4, 1, 0x60, 0, 0, 3, 2, 1, 0,
  0x0a, body.length + 2, 1, body.length, ...body,
]);

/** Probe actual capabilities, without fetching an unsupported artifact. */
export function runtimeCapabilities(host = globalThis) {
  let relaxedSimd = false;
  let sharedMemory = false;
  try { relaxedSimd = host.WebAssembly.validate(relaxedProbe); } catch { /* Use plain SIMD. */ }
  const isolated = host.crossOriginIsolated === true;
  const sharedArrayBuffer = typeof host.SharedArrayBuffer === 'function';
  if (isolated && sharedArrayBuffer) {
    try {
      const memory = new host.WebAssembly.Memory({initial: 1, maximum: 1, shared: true});
      sharedMemory = memory.buffer instanceof host.SharedArrayBuffer;
    } catch { /* An isolated page may still lack shared WASM memory. */ }
  }
  return {relaxedSimd, isolated, sharedArrayBuffer, sharedMemory, hardwareConcurrency: host.navigator?.hardwareConcurrency};
}

export function cpuThreadOption(options = {}) {
  if (!options || typeof options !== 'object' || Array.isArray(options)) {
    throw new TypeError('Runtime options must be an object.');
  }
  const threads = options.cpuThreads === undefined ? 'auto' : options.cpuThreads;
  if (threads === 'auto') return threads;
  if (!Number.isSafeInteger(threads) || threads < 1 || threads > 32) {
    throw new RangeError("cpuThreads must be 'auto' or an integer between 1 and 32.");
  }
  return threads;
}

/** One immutable runtime per worker. Dependency injection keeps capability and
 * startup-failure tests independent of a browser or generated WASM files. */
export function createRuntimeLoader({
  importModule = path => import(path),
  capabilities = () => runtimeCapabilities(),
  config = runtimeConfig,
} = {}) {
  let startup;
  let selectedThreads;
  return function load(options = {}, {workerOwner, onWorkerError} = {}) {
    let threads;
    try {
      threads = cpuThreadOption(options);
      if (startup && threads !== selectedThreads) {
        throw new Error('cpuThreads cannot change after runtime initialization starts; create another worker.');
      }
    } catch (error) { return Promise.reject(error); }
    if (startup) return startup;
    selectedThreads = threads;
    startup = (async () => {
      const caps = capabilities();
      const reported = caps.hardwareConcurrency;
      const automatic = threads === 'auto';
      if (automatic) threads = Number.isSafeInteger(reported) && reported > 0
        ? Math.max(1, Math.min(4, Math.floor(reported / 2))) : 1;
      const failures = [];
      const finish = (wasm, variant, count, terminateWorkers = () => {}) => {
        let disposed = false;
        return Object.freeze({
          loadModel: wasm.loadModel,
          DictationSession: wasm.DictationSession,
          variant,
          cpuThreads: count,
          fallbackReason: failures.length ? failures.join('; ') : undefined,
          // The owner must first finish active calls and free models/sessions.
          dispose() {
            if (disposed) return;
            disposed = true;
            return terminateWorkers();
          },
        });
      };
      const validate = wasm => {
        if (typeof wasm?.default !== 'function' || typeof wasm.loadModel !== 'function'
            || typeof wasm.DictationSession !== 'function') {
          throw new Error('Runtime is missing required exports.');
        }
      };
      if (threads > 1) {
        if (!config.threadedAvailable) {
          if (!automatic) failures.push('Threaded CPU artifact is not installed; using one thread.');
        } else if (!workerOwner || typeof workerOwner.spawn !== 'function' || typeof workerOwner.dispose !== 'function') {
          failures.push('Threaded CPU requires an independent worker owner; using one thread.');
        } else if (caps.isolated && caps.sharedArrayBuffer && caps.sharedMemory) {
          let pool;
          try {
            pool = await importModule('./threaded/thread-pool.js');
            // Require cleanup before starting anything that might create workers.
            if (typeof pool.terminateWorkers !== 'function') throw new Error('Threaded runtime has no cleanup hook.');
            if (typeof pool.requiresRelaxedSimd !== 'boolean') throw new Error('Threaded runtime is missing its SIMD requirement.');
            if (pool.requiresRelaxedSimd && !caps.relaxedSimd) throw new Error('Threaded runtime requires unsupported relaxed SIMD.');
            validate(pool.wasm);
            if (typeof pool.wasm.initCpuThreadPool !== 'function') throw new Error('Threaded runtime has no pool initializer.');
            if (workerOwner) {
              if (typeof pool.setWorkerOwner !== 'function') throw new Error('Threaded runtime does not support external worker ownership.');
              pool.setWorkerOwner(workerOwner);
            }
            if (onWorkerError && typeof pool.setWorkerErrorHandler === 'function') pool.setWorkerErrorHandler(onWorkerError);
            await pool.wasm.default();
            await pool.wasm.initCpuThreadPool(threads);
            return finish(pool.wasm, 'threaded', threads, () => pool.terminateWorkers());
          } catch (error) {
            if (typeof pool?.terminateWorkers === 'function') {
              // If cleanup itself fails, do not silently leave orphan workers running.
              try { await pool.terminateWorkers(); }
              catch (cleanupError) {
                throw new Error(`Threaded runtime failed (${message(error)}); cleanup failed (${message(cleanupError)}).`);
              }
            }
            failures.push(`Threaded CPU unavailable: ${message(error)}`);
          }
        } else failures.push('Threaded CPU requires cross-origin isolation and shared WASM memory; using one thread.');
      }
      if (caps.relaxedSimd) {
        try {
          const wasm = await importModule('./relaxed/speechworks_web.js');
          validate(wasm);
          await wasm.default();
          return finish(wasm, 'relaxed', 1);
        } catch (error) { failures.push(`Relaxed SIMD unavailable: ${message(error)}`); }
      }
      const wasm = await importModule('./speechworks_web.js');
      validate(wasm);
      await wasm.default();
      return finish(wasm, 'plain', 1);
    })();
    return startup;
  };
}

export const loadRuntime = createRuntimeLoader();
