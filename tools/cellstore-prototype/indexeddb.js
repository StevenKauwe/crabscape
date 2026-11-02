// Experimental IndexedDB protocol; not connected to gameplay or migration.
// One object store gives publication, staging, reset and collection a shared
// serializable transaction scope. Never share this database with live saves.
export async function openChunkPrototype(name) {
  const MAX_BATCH = 32, MAX_CHUNK_BYTES = 1024 * 1024, MAX_CHUNKS = 4096;
  const opening = indexedDB.open(name, 1);
  opening.onupgradeneeded = () => opening.result.createObjectStore('records');
  const db = await new Promise((resolve, reject) => {
    opening.onsuccess = () => resolve(opening.result);
    opening.onerror = () => reject(opening.error);
    opening.onblocked = () => reject(new Error('database blocked'));
  });
  const validId = id => typeof id === 'string' && /^[a-zA-Z0-9_-]{1,100}$/.test(id);
  const fail = (tx, reason) => { tx.reason = reason; tx.abort(); };
  const equal = (a, b) => a.length === b.length && a.every((v, i) => v === b[i]);
  function transact(mode, body) {
    return new Promise((resolve, reject) => {
      const tx = db.transaction('records', mode);
      let value;
      tx.oncomplete = () => resolve(value);
      tx.onabort = () => reject(new Error(tx.reason || tx.error?.message || 'transaction aborted'));
      tx.onerror = () => {}; // onabort reports the actual failure.
      try { body(tx, tx.objectStore('records'), v => { value = v; }); }
      catch (e) { fail(tx, e.message); }
    });
  }
  function metaAndStage(tx, store, id, epoch, f) {
    const m = store.get('meta'), s = store.get('stage:' + id);
    let count = 0;
    const ready = () => {
      if (++count !== 2) return;
      const meta = m.result || { epoch: 0, revision: 0 };
      if (meta.epoch !== epoch || !s.result || s.result.epoch !== epoch || s.result.kind !== 'writer') {
        fail(tx, 'stale writer'); return;
      }
      f(meta, s.result);
    };
    m.onsuccess = ready; s.onsuccess = ready;
  }
  return {
    close: () => db.close(),
    begin(id, tick, player, maxWriters = 1) {
      if (!validId(id) || !Number.isSafeInteger(tick) || tick < 0 || !(player instanceof Uint8Array) || player.length > MAX_CHUNK_BYTES || !Number.isSafeInteger(maxWriters) || maxWriters < 1 || maxWriters > 4) return Promise.reject(new Error('invalid input'));
      const frozenPlayer = player.slice();
      return transact('readwrite', (tx, store, done) => {
        const metaRequest = store.get('meta'), keys = store.getAllKeys();
        let count = 0;
        const ready = () => {
          if (++count !== 2) return;
          const meta = metaRequest.result || { epoch: 0, revision: 0 };
          const stages = keys.result.filter(k => typeof k === 'string' && k.startsWith('stage:'));
          if (stages.length >= maxWriters || stages.includes('stage:' + id)) { fail(tx, 'writer busy'); return; }
          store.put({ kind: 'writer', epoch: meta.epoch, revision: meta.revision, tick, player: frozenPlayer, chunks: [] }, 'stage:' + id);
          done(meta.epoch);
        };
        metaRequest.onsuccess = ready; keys.onsuccess = ready;
      });
    },
    stage(id, epoch, entries, maxBatch = 4) {
      if (!validId(id) || !Array.isArray(entries) || entries.length > maxBatch || !Number.isSafeInteger(maxBatch) || maxBatch < 1 || maxBatch > MAX_BATCH || entries.some(entry => !Array.isArray(entry) || entry.length !== 2 || !validId(entry[0]) || !(entry[1] instanceof Uint8Array) || entry[1].length > MAX_CHUNK_BYTES)) return Promise.reject(new Error('invalid batch'));
      // Freeze caller-owned arrays before the async transaction can observe them.
      const frozen = entries.map(([key, bytes]) => [key, bytes.slice()]);
      return transact('readwrite', (tx, store) => metaAndStage(tx, store, id, epoch, (meta, stage) => {
        if (stage.chunks.length + frozen.length > MAX_CHUNKS) { fail(tx, 'too many chunks'); return; }
        const seen = new Set(stage.chunks);
        for (const [key] of frozen) {
          if (seen.has(key)) { fail(tx, 'duplicate chunk'); return; }
          seen.add(key);
        }
        let remaining = frozen.length;
        const finish = () => { stage.chunks.push(...frozen.map(([key]) => key)); store.put(stage, 'stage:' + id); };
        if (!remaining) { finish(); return; }
        for (const [key, bytes] of frozen) {
          const req = store.get('chunk:' + key);
          req.onsuccess = () => {
            if (req.result !== undefined && (!(req.result instanceof Uint8Array) || !equal(req.result, bytes))) { fail(tx, 'immutable chunk mismatch'); return; }
            if (req.result === undefined) store.put(bytes, 'chunk:' + key);
            if (--remaining === 0) finish();
          };
        }
      }));
    },
    publish(id, epoch, expectedCount) {
      if (!validId(id) || !Number.isSafeInteger(expectedCount) || expectedCount < 0) return Promise.reject(new Error('invalid input'));
      return transact('readwrite', (tx, store) => metaAndStage(tx, store, id, epoch, (meta, stage) => {
        if (stage.revision !== meta.revision || stage.chunks.length !== expectedCount) { fail(tx, 'stale or incomplete snapshot'); return; }
        let remaining = stage.chunks.length;
        const finish = () => {
          meta.revision++;
          store.put(meta, 'meta');
          store.put({ revision: meta.revision, tick: stage.tick, player: stage.player, chunks: stage.chunks }, 'head');
          store.delete('stage:' + id);
        };
        if (!remaining) { finish(); return; }
        for (const key of stage.chunks) {
          const req = store.getKey('chunk:' + key);
          req.onsuccess = () => { if (req.result === undefined) { fail(tx, 'missing chunk'); return; } if (--remaining === 0) finish(); };
        }
      }));
    },
    abandon(id, epoch) {
      if (!validId(id)) return Promise.reject(new Error('invalid input'));
      return transact('readwrite', (tx, store) => metaAndStage(tx, store, id, epoch, () => store.delete('stage:' + id)));
    },
    collect() {
      return transact('readwrite', (_tx, store, done) => {
        const keys = store.getAllKeys(), head = store.get('head');
        const stages = store.getAll(IDBKeyRange.bound('stage:', 'stage:\uffff'));
        let count = 0;
        const ready = () => {
          if (++count !== 3) return;
          const retained = new Set();
          for (const manifest of [head.result, ...stages.result])
            if (manifest) for (const chunk of manifest.chunks) retained.add('chunk:' + chunk);
          let deleted = 0;
          for (const key of keys.result) if (typeof key === 'string' && key.startsWith('chunk:') && !retained.has(key)) { store.delete(key); deleted++; }
          done(deleted);
        };
        keys.onsuccess = ready; head.onsuccess = ready; stages.onsuccess = ready;
      });
    },
    reset() {
      return transact('readwrite', (_tx, store) => {
        const req = store.get('meta');
        req.onsuccess = () => {
          const meta = req.result || { epoch: 0, revision: 0 };
          store.clear();
          store.put({ epoch: meta.epoch + 1, revision: meta.revision + 1 }, 'meta');
        };
      });
    },
    // A pin retains the exact manifest during multiple bounded readonly
    // transactions. GC treats it like a stage; reset invalidates it by epoch.
    pin(id) {
      if (!validId(id)) return Promise.reject(new Error('invalid input'));
      return transact('readwrite', (tx, store, done) => {
        const m = store.get('meta'), h = store.get('head'), existing = store.get('stage:' + id);
        let n = 0;
        const ready = () => {
          if (++n !== 3) return;
          if (!h.result || existing.result) { fail(tx, 'missing head or duplicate pin'); return; }
          const meta = m.result || { epoch: 0, revision: 0 };
          const pin = { ...h.result, kind: 'pin', epoch: meta.epoch };
          store.put(pin, 'stage:' + id);
          done({ epoch: pin.epoch, tick: pin.tick, player: pin.player, count: pin.chunks.length });
        };
        m.onsuccess = ready; h.onsuccess = ready; existing.onsuccess = ready;
      });
    },
    readPage(id, epoch, offset, limit) {
      if (!validId(id) || !Number.isSafeInteger(offset) || offset < 0 || !Number.isSafeInteger(limit) || limit < 1 || limit > 32) return Promise.reject(new Error('invalid page'));
      return transact('readonly', (tx, store, done) => {
        const m = store.get('meta'), s = store.get('stage:' + id);
        let n = 0;
        const ready = () => {
          if (++n !== 2) return;
          const meta = m.result || { epoch: 0, revision: 0 }, pin = s.result;
          if (!pin || pin.kind !== 'pin' || pin.epoch !== epoch || meta.epoch !== epoch || offset > pin.chunks.length) { fail(tx, 'stale pin'); return; }
          const keys = pin.chunks.slice(offset, offset + limit), values = new Array(keys.length);
          let remaining = keys.length;
          if (!remaining) { done([]); return; }
          keys.forEach((key, i) => {
            const req = store.get('chunk:' + key);
            req.onsuccess = () => {
              if (!(req.result instanceof Uint8Array)) { fail(tx, 'missing chunk'); return; }
              values[i] = { key, bytes: req.result };
              if (--remaining === 0) done(values);
            };
          });
        };
        m.onsuccess = ready; s.onsuccess = ready;
      });
    },
    unpin(id, epoch) {
      if (!validId(id)) return Promise.reject(new Error('invalid input'));
      return transact('readwrite', (tx, store) => {
        const m = store.get('meta'), s = store.get('stage:' + id);
        let n = 0;
        const ready = () => {
          if (++n !== 2) return;
          const meta = m.result || { epoch: 0, revision: 0 };
          if (!s.result || s.result.kind !== 'pin' || s.result.epoch !== epoch || meta.epoch !== epoch) { fail(tx, 'stale pin'); return; }
          store.delete('stage:' + id);
        };
        m.onsuccess = ready; s.onsuccess = ready;
      });
    },
    read() {
      return transact('readonly', (tx, store, done) => {
        const req = store.get('head');
        req.onsuccess = () => {
          const head = req.result;
          if (!head) { done(null); return; }
          const chunks = new Array(head.chunks.length);
          let remaining = chunks.length;
          if (!remaining) { done({ head, chunks }); return; }
          head.chunks.forEach((key, i) => {
            const part = store.get('chunk:' + key);
            part.onsuccess = () => { if (!(part.result instanceof Uint8Array)) { fail(tx, 'missing chunk'); return; } chunks[i] = part.result; if (--remaining === 0) done({ head, chunks }); };
          });
        };
      });
    }
  };
}
