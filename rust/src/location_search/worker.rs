//! Latest-query mailbox: one active request, one pending query, one result.
//! Replacing/closing a search cancels queued work and suppresses stale results.
use super::{Client, Location};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};
struct Work {
    ticket: u64,
    query: String,
    cancel: Arc<AtomicBool>,
}
pub struct Ready {
    pub ticket: u64,
    pub result: Result<Vec<Location>, String>,
}
#[derive(Default)]
struct State {
    latest: u64,
    closed: bool,
    pending: Option<Work>,
    active: Option<Arc<AtomicBool>>,
    ready: Option<Ready>,
}
type Shared = Arc<(Mutex<State>, Condvar)>;
pub struct Worker {
    shared: Shared,
}
impl Worker {
    pub fn new() -> Result<Self, String> {
        let client = Client::new()?;
        Self::with_search(move |query, cancel| client.search(query, 6, cancel))
    }
    /// Injectable provider also permits deterministic offline workflow audits.
    pub fn with_search(
        mut search: impl FnMut(&str, &AtomicBool) -> Result<Vec<Location>, String> + Send + 'static,
    ) -> Result<Self, String> {
        let shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let worker = shared.clone();
        std::thread::Builder::new()
            .name("SleufBase-location".into())
            .spawn(move || loop {
                let (mutex, wake) = &*worker;
                let Ok(mut state) = mutex.lock() else {
                    return;
                };
                while state.pending.is_none() && !state.closed {
                    state = match wake.wait(state) {
                        Ok(s) => s,
                        Err(_) => return,
                    };
                }
                if state.closed {
                    return;
                }
                let work = state.pending.take().unwrap();
                state.active = Some(work.cancel.clone());
                drop(state);
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    search(&work.query, &work.cancel)
                }))
                .unwrap_or_else(|_| Err("Locatiezoeker werd onverwacht onderbroken".into()));
                let Ok(mut state) = mutex.lock() else {
                    return;
                };
                state.active = None;
                if !state.closed
                    && !work.cancel.load(Ordering::Relaxed)
                    && work.ticket == state.latest
                {
                    state.ready = Some(Ready {
                        ticket: work.ticket,
                        result,
                    });
                }
            })
            .map_err(|e| format!("Locatiezoeker kon niet starten: {e}"))?;
        Ok(Self { shared })
    }
    pub fn search(&self, query: String) -> Result<u64, String> {
        super::bounded_query(&query)?;
        let mut state = self
            .shared
            .0
            .lock()
            .map_err(|_| "Locatiezoeker is onderbroken")?;
        if state.closed {
            return Err("Locatiezoeker is gesloten".into());
        }
        state.latest = state
            .latest
            .checked_add(1)
            .ok_or("Locatiezoekteller is vol")?;
        cancel(&mut state);
        let ticket = state.latest;
        state.pending = Some(Work {
            ticket,
            query,
            cancel: Arc::new(AtomicBool::new(false)),
        });
        self.shared.1.notify_one();
        Ok(ticket)
    }
    pub fn cancel(&self) {
        if let Ok(mut state) = self.shared.0.lock() {
            cancel(&mut state);
        }
    }
    pub fn poll(&self) -> Option<Ready> {
        self.shared.0.lock().ok()?.ready.take()
    }
}
fn cancel(state: &mut State) {
    if let Some(active) = &state.active {
        active.store(true, Ordering::Relaxed);
    }
    if let Some(work) = state.pending.take() {
        work.cancel.store(true, Ordering::Relaxed);
    }
    state.ready = None;
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Ok(mut state) = self.shared.0.lock() {
            state.closed = true;
            cancel(&mut state);
            self.shared.1.notify_one();
        }
        // Never block the UI on a socket: the bounded client request times out,
        // then observes cancellation. Only this worker owns its HTTP session.
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };
    #[test]
    fn rapid_replacements_keep_only_latest_query_and_closing_cancels_active_work() {
        let (started, rx) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let worker = Worker::with_search(move |query, cancel| {
            started.send(query.to_owned()).unwrap();
            gate.recv_timeout(Duration::from_secs(3)).unwrap();
            if query == "first" {
                assert!(cancel.load(Ordering::Relaxed));
            }
            Ok(vec![Location {
                label: query.into(),
                x: 155000.,
                y: 463000.,
                location_type: "adres".into(),
            }])
        })
        .unwrap();
        worker.search("first".into()).unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(3)).unwrap(), "first");
        for n in 0..1000 {
            worker.search(format!("discarded-{n}")).unwrap();
        }
        let ticket = worker.search("latest".into()).unwrap();
        release.send(()).unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(3)).unwrap(), "latest");
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let ready = loop {
            if let Some(r) = worker.poll() {
                break r;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(ready.ticket, ticket);
        assert_eq!(ready.result.unwrap()[0].label, "latest");
        worker.search("closing".into()).unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(3)).unwrap(), "closing");
        worker.cancel();
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while worker.shared.0.lock().unwrap().active.is_some() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(worker.poll().is_none());
    }
}
