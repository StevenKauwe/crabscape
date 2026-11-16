// Bounded entries avoid browsers rejecting a single ~296 MB cached response.
// Write the completion marker last. The caller verifies the full pinned hash.
const CHUNK = 64 * 1024 * 1024;
const HEADER = 'x-speechworks-chunks';
function partUrl(url, hash, index) {
  const key = new URL(url);
  key.searchParams.set('speechworks-part', `${hash}:${index}`);
  return key.href;
}
async function entries(cache, url, spec) {
  const response = await cache?.match(url);
  if (!response) return null;
  const marker = response.headers.get(HEADER);
  if (!marker) return [response];
  let info;
  try { info = JSON.parse(marker); } catch { return null; }
  if (info.bytes !== spec.bytes || info.sha256 !== spec.sha256 || info.chunk !== CHUNK) return null;
  const parts = [];
  for (let i = 0; i < Math.ceil(spec.bytes / CHUNK); i++) {
    const part = await cache.match(partUrl(url, spec.sha256, i));
    if (!part || Number(part.headers.get('content-length')) !== Math.min(CHUNK, spec.bytes - i * CHUNK)) return null;
    parts.push(part);
  }
  return parts;
}
export async function hasCachedModel(cache, url, spec) {
  return !!(await entries(cache, url, spec));
}
export async function cachedModel(cache, url, spec) {
  const parts = await entries(cache, url, spec);
  if (!parts) return null;
  if (parts.length === 1) return parts[0];
  let index = 0, reader;
  return new Response(new ReadableStream({
    async pull(controller) {
      while (index < parts.length) {
        reader ??= parts[index].body.getReader();
        const {value, done} = await reader.read();
        if (!done) { controller.enqueue(value); return; }
        reader.releaseLock(); reader = null; index++;
      }
      controller.close();
    },
    async cancel(reason) { await reader?.cancel(reason); },
  }), {headers: {'Content-Type': 'application/octet-stream'}});
}
export async function deleteCachedModel(cache, url, spec) {
  if (!cache) return;
  await cache.delete(url);
  for (let i = 0; i < Math.ceil(spec.bytes / CHUNK); i++) await cache.delete(partUrl(url, spec.sha256, i));
}
export async function cacheModel(cache, url, spec, bytes) {
  if (!cache) return;
  if (bytes.length <= CHUNK) {
    await cache.put(url, new Response(bytes, {headers: {'Content-Type': 'application/octet-stream'}}));
    return;
  }
  try {
    for (let at = 0, i = 0; at < bytes.length; at += CHUNK, i++) {
      const part = bytes.subarray(at, at + CHUNK);
      await cache.put(partUrl(url, spec.sha256, i), new Response(part, {
        headers: {'Content-Type': 'application/octet-stream', 'Content-Length': String(part.length)},
      }));
    }
    await cache.put(url, new Response(null, {headers: {
      [HEADER]: JSON.stringify({bytes: spec.bytes, sha256: spec.sha256, chunk: CHUNK}),
    }}));
  } catch (error) {
    await deleteCachedModel(cache, url, spec).catch(() => {});
    throw error;
  }
}
