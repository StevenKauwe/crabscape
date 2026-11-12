use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Arc, Mutex,
    },
};
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    pub executable: String,
    pub args: Vec<String>,
    pub workspace: String,
}
impl Registration {
    pub fn validate(&self) -> Result<(), String> {
        for p in [&self.executable, &self.workspace] {
            if !Path::new(p).is_absolute() {
                return Err("Registration paths must be absolute".into());
            }
        }
        if !Path::new(&self.executable).is_file() || !Path::new(&self.workspace).is_dir() {
            return Err("Registered executable/workspace missing".into());
        }
        if self.args.len() > 64 || self.args.iter().any(|s| s.len() > 8192) {
            return Err("Registration argument limit".into());
        }
        Ok(())
    }
}
// Includes active PTYs, blocked I/O workers and pending immediate-child cleanup.
// A tool leaving descendants alive may retain a slot rather than allow unbounded workers.
const MAX_LIFECYCLES: usize = 16;
static LIVE_LIFECYCLES: AtomicUsize = AtomicUsize::new(0);
struct Lifecycle(&'static AtomicUsize);
impl Lifecycle {
    fn acquire() -> Result<Arc<Self>, String> {
        Self::acquire_from(&LIVE_LIFECYCLES, MAX_LIFECYCLES)
    }
    fn acquire_from(counter: &'static AtomicUsize, limit: usize) -> Result<Arc<Self>, String> {
        counter
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                (n < limit).then_some(n + 1)
            })
            .map_err(|_| "PTY cleanup capacity reached; wait for prior tools to exit")?;
        Ok(Arc::new(Self(counter)))
    }
}
impl Drop for Lifecycle {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
struct Output {
    bytes: VecDeque<u8>,
    base: u64,
    ended: bool,
}
pub struct Terminal {
    lifecycle: Arc<Lifecycle>,
    _master: Box<dyn MasterPty + Send>,
    writer: mpsc::SyncSender<Vec<u8>>,
    child: Option<Box<dyn portable_pty::Child + Send + Sync>>,
    output: Arc<Mutex<Output>>,
}
impl Terminal {
    pub fn start(reg: &Registration) -> Result<Self, String> {
        reg.validate()?;
        let lifecycle = Lifecycle::acquire()?;
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 30,
                cols: 100,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|_| "PTY unavailable")?;
        let mut cmd = CommandBuilder::new(&reg.executable);
        cmd.args(&reg.args);
        cmd.cwd(&reg.workspace);
        cmd.env("TERM", "xterm-256color");
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|_| "PTY reader failed")?;
        let mut writer = pair.master.take_writer().map_err(|_| "PTY writer failed")?;
        // Bounded queue prevents a non-reading tool from blocking the server or revocation.
        let (sender, receiver) = mpsc::sync_channel::<Vec<u8>>(8);
        let input_lifecycle = lifecycle.clone();
        std::thread::Builder::new()
            .name("harbor-input".into())
            .spawn(move || {
                let _lifecycle = input_lifecycle;
                while let Ok(bytes) = receiver.recv() {
                    if writer.write_all(&bytes).is_err() {
                        break;
                    }
                }
            })
            .map_err(|_| "Could not start input worker")?;
        let output = Arc::new(Mutex::new(Output {
            bytes: VecDeque::new(),
            base: 0,
            ended: false,
        }));
        let shared = output.clone();
        let output_lifecycle = lifecycle.clone();
        std::thread::Builder::new()
            .name("harbor-output".into())
            .spawn(move || {
                let _lifecycle = output_lifecycle;
                let mut buf = [0; 4096];
                while let Ok(n) = reader.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    let Ok(mut o) = shared.lock() else {
                        return;
                    };
                    o.bytes.extend(&buf[..n]);
                    while o.bytes.len() > 262144 {
                        o.bytes.pop_front();
                        o.base += 1;
                    }
                }
                if let Ok(mut o) = shared.lock() {
                    o.ended = true;
                }
            })
            .map_err(|_| "Could not start output worker")?;
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|_| "Registered tool launch failed")?;
        drop(pair.slave);
        Ok(Self {
            lifecycle,
            _master: pair.master,
            writer: sender,
            child: Some(child),
            output,
        })
    }
    pub fn input(&mut self, text: &str) -> Result<(), String> {
        self.writer
            .try_send(text.as_bytes().to_vec())
            .map_err(|_| "Tool input queue full or closed".into())
    }
    pub fn output(&self, offset: u64) -> serde_json::Value {
        let o = self.output.lock().expect("output mutex");
        let start = offset.saturating_sub(o.base) as usize;
        let bytes: Vec<u8> = o.bytes.iter().skip(start).take(1024).copied().collect();
        serde_json::json!({"text":String::from_utf8_lossy(&bytes),"bytes":bytes,"offset":o.base+start.min(o.bytes.len()) as u64+bytes.len() as u64,"truncated":offset<o.base,"ended":o.ended && start.saturating_add(bytes.len())>=o.bytes.len()})
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // Revocation removes authority immediately; OS child cleanup cannot block state lock.
            let lifecycle = self.lifecycle.clone();
            std::thread::spawn(move || {
                let _lifecycle = lifecycle;
                let _ = child.kill();
                let _ = child.wait();
            });
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lifecycle_budget_retained_until_every_worker_exits() {
        static TEST: AtomicUsize = AtomicUsize::new(0);
        let first = Lifecycle::acquire_from(&TEST, 2).unwrap();
        let second = Lifecycle::acquire_from(&TEST, 2).unwrap();
        let blocked_worker = first.clone();
        drop(first);
        assert!(Lifecycle::acquire_from(&TEST, 2).is_err());
        drop(blocked_worker);
        let replacement = Lifecycle::acquire_from(&TEST, 2).unwrap();
        assert_eq!(TEST.load(Ordering::SeqCst), 2);
        drop(second);
        drop(replacement);
        assert_eq!(TEST.load(Ordering::SeqCst), 0);
    }
    #[test]
    fn rejects_relative_registration() {
        assert!(Registration {
            executable: "sh".into(),
            args: vec![],
            workspace: "/tmp".into()
        }
        .validate()
        .is_err());
    }
    #[test]
    fn real_pty_echo() {
        let r = Registration {
            executable: "/bin/cat".into(),
            args: vec![],
            workspace: "/tmp".into(),
        };
        let mut t = Terminal::start(&r).unwrap();
        t.input("harbor-test\n").unwrap();
        let mut seen = false;
        for _ in 0..30 {
            std::thread::sleep(std::time::Duration::from_millis(20));
            if t.output(0)["text"]
                .as_str()
                .unwrap()
                .contains("harbor-test")
            {
                seen = true;
                break;
            }
        }
        assert!(seen);
    }
}
