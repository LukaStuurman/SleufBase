//! One active job, one latest pending job and one result, shared by small
//! network workflows. Replacements cancel work and suppress stale publication.
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Condvar, Mutex,
};
struct Work<I> {
    id: u64,
    input: I,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicUsize>,
}
pub struct Ready<O> {
    pub ticket: u64,
    pub result: Result<O, String>,
}
pub(crate) struct Ticket {
    pub id: u64,
    pub progress: Arc<AtomicUsize>,
}
struct State<I, O> {
    latest: u64,
    closed: bool,
    pending: Option<Work<I>>,
    active: Option<Arc<AtomicBool>>,
    ready: Option<Ready<O>>,
}
impl<I, O> Default for State<I, O> {
    fn default() -> Self {
        Self {
            latest: 0,
            closed: false,
            pending: None,
            active: None,
            ready: None,
        }
    }
}
type Shared<I, O> = Arc<(Mutex<State<I, O>>, Condvar)>;
pub(crate) struct Worker<I, O> {
    shared: Shared<I, O>,
}
impl<I: Send + 'static, O: Send + 'static> Worker<I, O> {
    pub fn new(
        name: &str,
        mut run: impl FnMut(I, &AtomicBool, &AtomicUsize) -> Result<O, String> + Send + 'static,
    ) -> Result<Self, String> {
        let shared = Arc::new((Mutex::new(State::<I, O>::default()), Condvar::new()));
        let worker = shared.clone();
        std::thread::Builder::new()
            .name(name.into())
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
                    run(work.input, &work.cancel, &work.progress)
                }))
                .unwrap_or_else(|_| Err("Achtergrondaanvraag werd onverwacht onderbroken".into()));
                let Ok(mut state) = mutex.lock() else {
                    return;
                };
                state.active = None;
                if !state.closed && !work.cancel.load(Ordering::Relaxed) && work.id == state.latest
                {
                    state.ready = Some(Ready {
                        ticket: work.id,
                        result,
                    });
                }
            })
            .map_err(|e| format!("Achtergrondaanvraag kon niet starten: {e}"))?;
        Ok(Self { shared })
    }
    pub fn submit(&self, input: I) -> Result<Ticket, String> {
        let mut state = self
            .shared
            .0
            .lock()
            .map_err(|_| "Achtergrondaanvraag is onderbroken")?;
        if state.closed {
            return Err("Achtergrondaanvraag is gesloten".into());
        }
        state.latest = state.latest.checked_add(1).ok_or("Aanvraagteller is vol")?;
        cancel(&mut state);
        let id = state.latest;
        let progress = Arc::new(AtomicUsize::new(0));
        state.pending = Some(Work {
            id,
            input,
            cancel: Arc::new(AtomicBool::new(false)),
            progress: progress.clone(),
        });
        self.shared.1.notify_one();
        Ok(Ticket { id, progress })
    }
    pub fn cancel(&self) {
        if let Ok(mut state) = self.shared.0.lock() {
            cancel(&mut state);
        }
    }
    pub fn poll(&self) -> Option<Ready<O>> {
        self.shared.0.lock().ok()?.ready.take()
    }
    #[cfg(test)]
    pub fn is_idle(&self) -> bool {
        self.shared
            .0
            .lock()
            .is_ok_and(|s| s.active.is_none() && s.pending.is_none())
    }
}
fn cancel<I, O>(state: &mut State<I, O>) {
    if let Some(active) = &state.active {
        active.store(true, Ordering::Relaxed);
    }
    if let Some(work) = state.pending.take() {
        work.cancel.store(true, Ordering::Relaxed);
    }
    state.ready = None;
}
impl<I, O> Drop for Worker<I, O> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.shared.0.lock() {
            state.closed = true;
            cancel(&mut state);
            self.shared.1.notify_one();
        }
        // A bounded socket timeout may finish after the UI has closed. Never
        // join that socket on the UI thread; it cannot publish after closing.
    }
}
