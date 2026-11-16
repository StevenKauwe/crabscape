// Only the browser entry point belongs to the game. Speechworks owns inference,
// loading, scheduling and CPU/GPU selection; its entire bundle has an immutable URL.
try {
  const response = await fetch(new URL("./speechworks-version.txt", import.meta.url), { cache: "no-store" });
  if (!response.ok) throw new Error(`Speech runtime is unavailable (${response.status}).`);
  const version = (await response.text()).trim();
  if (!/^[a-f0-9]{40}-[a-f0-9]{64}$/.test(version)) throw new Error("Invalid speech runtime version.");
  const root = new URL(`./runtime/${version}/`, import.meta.url);
  const { startWorker } = await import(new URL("worker.js", root).href);
  await startWorker({ manifestUrl: new URL("models.json", root).href, backend: "auto" });
} catch (error) {
  self.postMessage({ type: "error", message: String(error?.message || error) });
}
