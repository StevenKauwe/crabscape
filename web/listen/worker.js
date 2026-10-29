// Starts the speech-to-text listener (crates/listen, Rust) in a background worker; it takes over from here.
// Its glue and .wasm are loaded with the build's stamp (listen-version.txt, always fetched fresh) so a cached
// old glue is never paired with a new .wasm (see the same note in index.html).
fetch("./listen-version.txt", { cache: "no-store" })
  .then((r) => (r.ok ? r.text() : ""), () => "")
  .then((v) => (v.trim() ? "?v=" + encodeURIComponent(v.trim()) : ""))
  .then((q) =>
    import("./listen.js" + q).then((glue) =>
      glue.default({ module_or_path: new URL("./listen_bg.wasm" + q, self.location.href) }),
    ),
  );
