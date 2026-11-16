// Hosts with a per-file limit can serve ordered pieces of the same pinned model.
// The worker still checks the full size and SHA-256 before loading or caching it.
export async function fetchModel(url, spec) {
  if (spec.parts === undefined) return fetch(url, {redirect: 'error'});
  if (!Array.isArray(spec.parts) || !spec.parts.length || spec.parts.length > 512) {
    throw new Error('Invalid model download parts');
  }
  const base = new URL(url);
  let total = 0;
  const parts = spec.parts.map(part => {
    if (!part || typeof part.url !== 'string' || !part.url
        || !Number.isSafeInteger(part.bytes) || part.bytes < 1) {
      throw new Error('Invalid model download part');
    }
    // Part URLs are relative to the canonical model URL, not the manifest.
    const source = new URL(part.url, base);
    if (source.origin !== base.origin) throw new Error('Model parts must be served by this application.');
    total += part.bytes;
    if (!Number.isSafeInteger(total) || total > spec.bytes) throw new Error('Model part sizes exceed the pinned size');
    return {url: source, bytes: part.bytes};
  });
  if (total !== spec.bytes) throw new Error('Model part sizes do not match the pinned size');
  let index = 0, reader, received = 0;
  const abort = new AbortController();
  return new Response(new ReadableStream({
    async pull(controller) {
      try {
        while (index < parts.length) {
          const part = parts[index];
          if (!reader) {
            const response = await fetch(part.url, {redirect: 'error', signal: abort.signal});
            if (!response.ok) throw new Error(`Model part ${index + 1}: HTTP ${response.status}`);
            if (!response.body) throw new Error(`Model part ${index + 1}: empty response`);
            reader = response.body.getReader();
            received = 0;
          }
          const {value, done} = await reader.read();
          if (!done) {
            received += value.length;
            if (received > part.bytes) throw new Error(`Model part ${index + 1} exceeds its pinned size`);
            controller.enqueue(value);
            return;
          }
          if (received !== part.bytes) throw new Error(`Model part ${index + 1} is incomplete`);
          reader.releaseLock(); reader = null; index++;
        }
        controller.close();
      } catch (error) {
        abort.abort();
        await reader?.cancel().catch(() => {});
        controller.error(error);
      }
    },
    async cancel(reason) { abort.abort(); await reader?.cancel(reason).catch(() => {}); },
  }), {headers: {'Content-Type': 'application/octet-stream'}});
}
