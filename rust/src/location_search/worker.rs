//! Latest-query worker shared with the TIFF-address workflow.
use super::{Client, Location};
use std::sync::atomic::AtomicBool;
pub type Ready = crate::latest_job::Ready<Vec<Location>>;
pub struct Worker {
    inner: crate::latest_job::Worker<String, Vec<Location>>,
}
impl Worker {
    pub fn new() -> Result<Self, String> {
        let client = Client::new()?;
        Self::with_search(move |query, cancel| client.search(query, 6, cancel))
    }
    pub fn with_search(
        mut search: impl FnMut(&str, &AtomicBool) -> Result<Vec<Location>, String> + Send + 'static,
    ) -> Result<Self, String> {
        Ok(Self {
            inner: crate::latest_job::Worker::new(
                "SleufBase-location",
                move |query: String, cancel, _| search(&query, cancel),
            )?,
        })
    }
    pub fn search(&self, query: String) -> Result<u64, String> {
        super::bounded_query(&query)?;
        Ok(self.inner.submit(query)?.id)
    }
    pub fn cancel(&self) {
        self.inner.cancel();
    }
    pub fn poll(&self) -> Option<Ready> {
        self.inner.poll()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
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
        while !worker.inner.is_idle() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(worker.poll().is_none());
    }
}
