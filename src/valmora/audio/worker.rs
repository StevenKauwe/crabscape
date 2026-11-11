//! One bounded native producer; no PCM samples are produced on the render thread.
use super::{Note, pcm};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::Instant,
};
struct Job {
    notes: Vec<Note>,
    submitted: Instant,
    generation: u64,
}
pub struct Ready {
    pub bytes: Arc<[u8]>,
    pub submitted: Instant,
    pub generation: u64,
    pub duration: f64,
}
pub struct Worker {
    jobs: SyncSender<Job>,
    results: Receiver<Ready>,
    generation: Arc<AtomicU64>,
}
impl Worker {
    pub fn new() -> Option<Self> {
        let (jobs, input) = mpsc::sync_channel::<Job>(2);
        let (output, results) = mpsc::sync_channel(2);
        let generation = Arc::new(AtomicU64::new(0));
        let worker_generation = generation.clone();
        std::thread::Builder::new()
            .name("valmora-music".into())
            .spawn(move || {
                while let Ok(job) = input.recv() {
                    if job.generation != worker_generation.load(Ordering::Relaxed) {
                        continue;
                    }
                    let bytes: Arc<[u8]> = pcm::wav(&job.notes).into();
                    let duration = (bytes.len().saturating_sub(44)) as f64 / 88200.;
                    if job.generation != worker_generation.load(Ordering::Relaxed) {
                        continue;
                    }
                    if output
                        .send(Ready {
                            bytes,
                            submitted: job.submitted,
                            generation: job.generation,
                            duration,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .ok()?;
        Some(Self {
            jobs,
            results,
            generation,
        })
    }
    pub fn submit(&self, notes: &[Note]) {
        if notes.is_empty() || notes.len() > 64 {
            return;
        }
        let job = Job {
            notes: notes.to_vec(),
            submitted: Instant::now(),
            generation: self.generation.load(Ordering::Relaxed),
        };
        // A full queue drops this lookahead batch rather than blocking a frame.
        let _ = self.jobs.try_send(job);
    }
    pub fn cancel(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }
    pub fn take(&self) -> Option<Ready> {
        // Bounded queue: at most two stale results are removed in one frame.
        for _ in 0..2 {
            let ready = self.results.try_recv().ok()?;
            if ready.generation == self.generation.load(Ordering::Relaxed) {
                return Some(ready);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synthesized_bytes_and_cancellation_generation_match() {
        let worker = Worker::new().expect("native audio worker");
        let notes =
            crate::valmora::audio::Synth::default().recipe(crate::valmora::audio::Effect::Pick);
        worker.submit(&notes);
        let ready = worker
            .results
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("rendered batch");
        assert_eq!(&*ready.bytes, pcm::wav(&notes));
        assert_eq!(ready.generation, 0);
        worker.cancel();
        worker.submit(&notes);
        let ready = worker
            .results
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("new generation");
        assert_eq!(ready.generation, 1);
    }
    #[test]
    fn oversized_batches_are_not_enqueued() {
        let worker = Worker::new().expect("native audio worker");
        let note = super::super::tone(440., 0.1, super::super::Wave::Sine, 0.1, None, 0.);
        worker.submit(&vec![note; 65]);
        assert!(worker.results.try_recv().is_err());
    }
}
