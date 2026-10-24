//! Cooperative cancellation with a single, linearized publication boundary.
use std::{
    io::{self, Seek, SeekFrom, Write},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
        Mutex,
    },
};
const RUNNING: u8 = 0;
const CANCELLED: u8 = 1;
const PUBLISHING: u8 = 2;
const DONE: u8 = 3;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Preparing,
    Planning,
    Writing,
    Publishing,
}
pub struct Control {
    state: AtomicU8,
    cancel: AtomicBool,
    stage: AtomicU8,
    bytes: AtomicU64,
    detail: Mutex<String>,
}
impl Default for Control {
    fn default() -> Self {
        Self {
            state: AtomicU8::new(RUNNING),
            cancel: AtomicBool::new(false),
            stage: AtomicU8::new(0),
            bytes: AtomicU64::new(0),
            detail: Mutex::new(String::new()),
        }
    }
}
impl Control {
    /// Observe cooperative cancellation in external worker adapters. Request
    /// cancellation through `cancel()` so the publication gate remains atomic.
    pub fn cancellation_flag(&self)->&std::sync::atomic::AtomicBool{self.flag()}
    pub fn detail(&self) -> String {
        self.detail.lock().map(|s| s.clone()).unwrap_or_default()
    }
    pub(crate) fn set_detail(&self, text: &str) {
        if let Ok(mut s) = self.detail.lock() {
            s.clear();
            s.extend(text.chars().take(2048));
        }
    }
    pub fn cancel(&self) -> bool {
        if self
            .state
            .compare_exchange(RUNNING, CANCELLED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.cancel.store(true, Ordering::Release);
            true
        } else {
            false
        }
    }
    pub fn is_cancelled(&self) -> bool {
        self.state.load(Ordering::Acquire) == CANCELLED
    }
    pub fn can_cancel(&self) -> bool {
        self.state.load(Ordering::Acquire) == RUNNING
    }
    pub fn check(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err("Export geannuleerd".into())
        } else {
            Ok(())
        }
    }
    pub(crate) fn flag(&self) -> &AtomicBool {
        &self.cancel
    }
    pub(crate) fn set_stage(&self, stage: Stage) {
        self.stage.store(stage as u8, Ordering::Relaxed);
    }
    pub fn progress(&self) -> (Stage, u64) {
        let stage = match self.stage.load(Ordering::Relaxed) {
            0 => Stage::Preparing,
            1 => Stage::Planning,
            2 => Stage::Writing,
            _ => Stage::Publishing,
        };
        (stage, self.bytes.load(Ordering::Relaxed))
    }
    pub(crate) fn begin_publish(&self) -> Result<(), String> {
        self.state
            .compare_exchange(RUNNING, PUBLISHING, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "Export geannuleerd".to_owned())?;
        self.set_stage(Stage::Publishing);
        Ok(())
    }
    pub(crate) fn finish(&self) {
        let _ = self
            .state
            .compare_exchange(RUNNING, DONE, Ordering::AcqRel, Ordering::Acquire);
        let _ = self
            .state
            .compare_exchange(PUBLISHING, DONE, Ordering::AcqRel, Ordering::Acquire);
    }
}
pub(crate) struct CheckedWriter<'a, W> {
    pub out: W,
    pub control: &'a Control,
}
impl<W: Write> Write for CheckedWriter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.control.is_cancelled() {
            return Err(io::Error::other("Export geannuleerd"));
        }
        let n = self.out.write(bytes)?;
        self.control.bytes.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}
impl<W: Seek> Seek for CheckedWriter<'_, W> {
    fn seek(&mut self, p: SeekFrom) -> io::Result<u64> {
        self.out.seek(p)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_and_publication_have_one_winner() {
        let cancelled = Control::default();
        assert!(cancelled.cancel());
        assert!(cancelled.begin_publish().is_err());
        let publishing = Control::default();
        publishing.begin_publish().unwrap();
        assert!(!publishing.cancel());
        assert!(!publishing.is_cancelled());
    }
}
