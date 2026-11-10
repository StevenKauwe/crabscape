//! Background work and file dialogs. No model fitting ever runs on Bevy's frame thread.
use king_ddd::{Generation, WorkRequest, WorkResult};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
pub enum Event {
    Generation(Generation),
    Done(Result<WorkResult, String>),
    Opened { bytes: Vec<u8>, name: String },
    Status(String),
}
#[derive(Default)]
pub struct Transport {
    pub events: Rc<RefCell<VecDeque<Event>>>,
    #[cfg(not(target_arch = "wasm32"))]
    rx: Option<std::sync::mpsc::Receiver<Event>>,
    #[cfg(not(target_arch = "wasm32"))]
    cancelled: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    #[cfg(target_arch = "wasm32")]
    worker: Option<BrowserWorker>,
}
impl Transport {
    pub fn poll(&mut self) -> Vec<Event> {
        #[cfg(not(target_arch = "wasm32"))]
        let mut finished = false;
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(rx) = &self.rx {
            while let Ok(e) = rx.try_recv() {
                finished |= matches!(&e, Event::Done(_));
                if self
                    .cancelled
                    .as_ref()
                    .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
                {
                    if finished {
                        self.events
                            .borrow_mut()
                            .push_back(Event::Done(Err("Tournament cancelled".into())));
                    }
                } else {
                    self.events.borrow_mut().push_back(e);
                }
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        if finished {
            self.rx = None;
            self.cancelled = None;
        }
        let events: Vec<_> = self.events.borrow_mut().drain(..).collect();
        #[cfg(target_arch = "wasm32")]
        if events.iter().any(|e| matches!(e, Event::Done(_))) {
            self.worker = None;
        }
        events
    }
    pub fn cancel(&mut self) {
        self.events.borrow_mut().clear();
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(flag) = &self.cancelled {
                flag.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.worker = None;
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run(&mut self, request: WorkRequest) {
        if self.rx.is_some() {
            self.events.borrow_mut().push_back(Event::Done(Err(
                "The previous Oracle is still stopping. Try again after its current fit finishes."
                    .into(),
            )));
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        self.rx = Some(rx);
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.cancelled = Some(flag.clone());
        std::thread::spawn(move || {
            let result = (|| {
                let reg = request
                    .columns
                    .iter()
                    .any(|c| c.role == king_ddd::Role::Output && c.kind == king_ddd::Kind::Number);
                let class = request.columns.iter().any(|c| {
                    c.role == king_ddd::Role::Output && c.kind == king_ddd::Kind::Category
                });
                let _ = tx.send(Event::Status("Reading the Oracle's real weights…".into()));
                let reg = if reg {
                    Some(weights("regressor")?)
                } else {
                    None
                };
                let class = if class {
                    Some(weights("classifier")?)
                } else {
                    None
                };
                king_ddd::run_cancellable(
                    &request,
                    reg,
                    class,
                    |g| {
                        let _ = tx.send(Event::Generation(g.clone()));
                    },
                    || flag.load(std::sync::atomic::Ordering::Relaxed),
                )
            })();
            let _ = tx.send(Event::Done(result));
        });
    }
    #[cfg(target_arch = "wasm32")]
    pub fn run(&mut self, request: WorkRequest) {
        self.cancel();
        match BrowserWorker::new(request, self.events.clone()) {
            Ok(w) => self.worker = Some(w),
            Err(e) => self.events.borrow_mut().push_back(Event::Done(Err(e))),
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Workbooks", &["xlsx", "csv", "tsv", "txt"])
            .pick_file()
        {
            match std::fs::read(&path) {
                Ok(bytes) => self.events.borrow_mut().push_back(Event::Opened {
                    bytes,
                    name: path
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "workbook.xlsx".into()),
                }),
                Err(e) => self
                    .events
                    .borrow_mut()
                    .push_back(Event::Status(e.to_string())),
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn export(&mut self, name: &str, bytes: &[u8]) {
        if let Some(path) = rfd::FileDialog::new().set_file_name(name).save_file()
            && let Err(e) = std::fs::write(path, bytes)
        {
            self.events
                .borrow_mut()
                .push_back(Event::Status(e.to_string()));
        }
    }
    #[cfg(target_arch = "wasm32")]
    pub fn open(&mut self) {
        if let Err(e) = browser_open(self.events.clone()) {
            self.events.borrow_mut().push_back(Event::Status(e));
        }
    }
    #[cfg(target_arch = "wasm32")]
    pub fn export(&mut self, name: &str, bytes: &[u8]) {
        if let Err(e) = browser_export(name, bytes) {
            self.events.borrow_mut().push_back(Event::Status(e));
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn weights(kind: &str) -> Result<Vec<u8>, String> {
    let base = format!("https://crabscape.com/king-ddd/vendor/engine/weights/{kind}-v2-half");
    let mut out = vec![];
    for i in 0..4 {
        let local = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("assets/king-ddd/weights/{kind}-v2-half.{i:02}.bin"));
        let bytes = if local.exists() {
            std::fs::read(local).map_err(|e| e.to_string())?
        } else {
            ureq::get(&format!("{base}.{i:02}.bin"))
                .call()
                .map_err(|e| e.to_string())?
                .body_mut()
                .with_config()
                .limit(16 * 1024 * 1024)
                .read_to_vec()
                .map_err(|e| e.to_string())?
        };
        out.extend(bytes);
    }
    if out.len() > 64 * 1024 * 1024 {
        return Err("Weights exceed memory budget".into());
    }
    Ok(out)
}
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, closure::Closure};
#[cfg(target_arch = "wasm32")]
struct BrowserWorker {
    worker: web_sys::Worker,
    _message: Closure<dyn FnMut(web_sys::MessageEvent)>,
    _error: Closure<dyn FnMut(web_sys::ErrorEvent)>,
}
#[cfg(target_arch = "wasm32")]
impl Drop for BrowserWorker {
    fn drop(&mut self) {
        self.worker.set_onmessage(None);
        self.worker.set_onerror(None);
        self.worker.terminate();
    }
}
#[cfg(target_arch = "wasm32")]
impl BrowserWorker {
    fn new(request: WorkRequest, events: Rc<RefCell<VecDeque<Event>>>) -> Result<Self, String> {
        let window = web_sys::window().ok_or("No browser")?;
        let path = window.location().pathname().map_err(|_| "No page path")?;
        let base = if path.ends_with('/') {
            path
        } else {
            path.rsplit_once('/')
                .map(|(p, _)| format!("{p}/"))
                .unwrap_or_else(|| "/".into())
        };
        let options = web_sys::WorkerOptions::new();
        options.set_type(web_sys::WorkerType::Module);
        let worker =
            web_sys::Worker::new_with_options(&format!("{base}kingworker/worker.js"), &options)
                .map_err(|_| "Could not start the Oracle worker")?;
        let reply = worker.clone();
        let json = serde_json::to_string(&request).map_err(|e| e.to_string())?;
        let queue = events.clone();
        let message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(
            move |event: web_sys::MessageEvent| {
                let Some(text) = event.data().as_string() else {
                    return;
                };
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
                    return;
                };
                match value.get("state").and_then(|v| v.as_str()) {
                    Some("ready") => {
                        if reply.post_message(&json.clone().into()).is_err() {
                            queue.borrow_mut().push_back(Event::Done(Err(
                                "Could not send the workbook to the Oracle".into(),
                            )));
                        }
                    }
                    Some("generation") => {
                        if let Ok(g) = serde_json::from_value(value["generation"].clone()) {
                            queue.borrow_mut().push_back(Event::Generation(g));
                        }
                    }
                    Some("done") => {
                        let r = serde_json::from_value(value["result"].clone())
                            .map_err(|e| e.to_string());
                        queue.borrow_mut().push_back(Event::Done(r));
                    }
                    Some("error") | Some("failed") => {
                        queue.borrow_mut().push_back(Event::Done(Err(value["error"]
                            .as_str()
                            .unwrap_or("Oracle failed")
                            .into())));
                    }
                    Some("status") => {
                        queue.borrow_mut().push_back(Event::Status(
                            value["text"].as_str().unwrap_or("Working…").into(),
                        ));
                    }
                    _ => {}
                }
            },
        );
        worker.set_onmessage(Some(message.as_ref().unchecked_ref()));
        let error =
            Closure::<dyn FnMut(web_sys::ErrorEvent)>::new(move |e: web_sys::ErrorEvent| {
                e.prevent_default();
                events
                    .borrow_mut()
                    .push_back(Event::Done(Err(format!("Oracle worker: {}", e.message()))));
            });
        worker.set_onerror(Some(error.as_ref().unchecked_ref()));
        Ok(Self {
            worker,
            _message: message,
            _error: error,
        })
    }
}
#[cfg(target_arch = "wasm32")]
fn browser_open(queue: Rc<RefCell<VecDeque<Event>>>) -> Result<(), String> {
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("No document")?;
    let input = document
        .create_element("input")
        .map_err(|_| "File chooser unavailable")?
        .dyn_into::<web_sys::HtmlInputElement>()
        .map_err(|_| "File chooser unavailable")?;
    input.set_type("file");
    input.set_accept(".xlsx,.csv,.tsv,.txt");
    let selected = input.clone();
    let callback = Closure::<dyn FnMut(web_sys::Event)>::once(move |_| {
        let Some(file) = selected.files().and_then(|f| f.get(0)) else {
            return;
        };
        let name = file.name();
        wasm_bindgen_futures::spawn_local(async move {
            if file.size() > 20_000_000. {
                queue
                    .borrow_mut()
                    .push_back(Event::Status("Workbook limit: 20 MB".into()));
                return;
            }
            match wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
                Ok(b) => queue.borrow_mut().push_back(Event::Opened {
                    bytes: js_sys::Uint8Array::new(&b).to_vec(),
                    name,
                }),
                Err(_) => queue
                    .borrow_mut()
                    .push_back(Event::Status("Could not read workbook".into())),
            }
        });
    });
    input.set_onchange(Some(callback.as_ref().unchecked_ref()));
    callback.forget();
    input.click();
    Ok(())
}
#[cfg(target_arch = "wasm32")]
fn browser_export(name: &str, bytes: &[u8]) -> Result<(), String> {
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("No document")?;
    let array = js_sys::Array::new();
    array.push(&js_sys::Uint8Array::from(bytes));
    let blob = web_sys::Blob::new_with_u8_array_sequence(&array)
        .map_err(|_| "Could not create workbook")?;
    let url =
        web_sys::Url::create_object_url_with_blob(&blob).map_err(|_| "Download unavailable")?;
    let link = document
        .create_element("a")
        .map_err(|_| "Download unavailable")?
        .dyn_into::<web_sys::HtmlAnchorElement>()
        .map_err(|_| "Download unavailable")?;
    link.set_href(&url);
    link.set_download(name);
    link.click();
    let _ = web_sys::Url::revoke_object_url(&url);
    Ok(())
}
