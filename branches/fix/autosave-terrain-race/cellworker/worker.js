// Starts the Rust cellular checkpoint worker. Fetch its stamp afresh so a
// cached glue module is never paired with a different build's Wasm binary.
fetch("./cellworker-version.txt", { cache: "no-store" })
  .then((r) => (r.ok ? r.text() : ""), () => "")
  .then((v) => (v.trim() ? "?v=" + encodeURIComponent(v.trim()) : ""))
  .then((q) =>
    import("./cellworker.js" + q).then((glue) =>
      glue.default({ module_or_path: new URL("./cellworker_bg.wasm" + q, self.location.href) }),
    ),
  )
  .catch((error) => self.postMessage({ state: "failed", error: String(error) }));
