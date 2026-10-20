//! One export worker. Inputs are moved, never cloned, and always returned.
use crate::{
    bgt::BgtLayer, cadastral::CadastralLayer, disciplines::Rule, export_control::Control,
    object_dxf::Options, object_selection::Selection, virtual_trench::VirtualTrench, Dataset,
};
use std::{
    path::PathBuf,
    sync::{
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
        Arc,
    },
};
#[derive(Default, Debug)]
pub struct Inputs {
    pub datasets: Vec<Dataset>,
    pub trenches: Vec<VirtualTrench>,
    pub cadastral: Option<CadastralLayer>,
    pub bgt: Vec<BgtLayer>,
    pub rasters: Vec<crate::geotiff::GeoTiffInfo>,
    pub overlays: Vec<crate::dxf_read::DxfOverlay>,
    pub comments: Vec<crate::project::Comment>,
    /// Project containers / imported archives that must never become DXF output.
    pub protected_paths: Vec<PathBuf>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Dxf,
    Xlsx,
    Template,
}
pub struct Request {
    pub kind: Kind,
    pub path: PathBuf,
    pub selection: Selection,
    pub rules: Vec<Rule>,
    pub extra_layers: bool,
    pub options: Options,
    pub template: Option<Box<crate::template_order::State>>,
    pub discipline_excel: bool,
}
#[derive(Debug)]
pub struct Exported {
    pub skipped: Vec<String>,
    pub companion: Option<PathBuf>,
    pub warnings: Vec<String>,
}
pub struct Ready {
    pub inputs: Inputs,
    pub result: Result<Exported, String>,
}
#[derive(Debug)]
pub struct StartFailure {
    pub inputs: Inputs,
    pub message: String,
}
struct Work {
    inputs: Inputs,
    request: Request,
    control: Arc<Control>,
}
pub struct Exporter {
    requests: SyncSender<Work>,
    results: Receiver<Ready>,
    active: Option<Arc<Control>>,
    closed: bool,
}
/// Shared by production and injected-worker regression tests.
#[doc(hidden)]
pub fn process(inputs: &Inputs, request: &Request, control: &Control) -> Result<Exported, String> {
    control.check()?;
    if request.kind == Kind::Template {
        let state = request
            .template
            .as_deref()
            .ok_or("Sjablooninstellingen ontbreken")?;
        let exported = crate::template_workflow::export_with_excel(
            &request.path,
            state,
            inputs,
            &request.rules,
            request.discipline_excel,
            control,
        )?;
        return Ok(Exported {
            skipped: Vec::new(),
            companion: exported.companion,
            warnings: exported.warnings,
        });
    }
    let prepared = request.selection.prepare_with_cancel(
        &inputs.datasets,
        &inputs.trenches,
        control.flag(),
    )?;
    if prepared.is_empty() && (request.kind == Kind::Xlsx || !request.extra_layers) {
        return Err(
            "Geen bruikbare objecten geselecteerd. Kies een job of neem virtuele eindpunten mee."
                .into(),
        );
    }
    match request.kind {
        Kind::Dxf => {
            let trenches = if request.extra_layers {
                inputs.trenches.as_slice()
            } else {
                &[]
            };
            let cad = if request.extra_layers {
                inputs.cadastral.as_ref()
            } else {
                None
            };
            let bgt = if request.extra_layers {
                inputs.bgt.as_slice()
            } else {
                &[]
            };
            crate::object_dxf::write_controlled(
                &request.path,
                prepared.datasets(),
                crate::object_dxf::Layers { trenches, cad, bgt },
                &request.rules,
                request.options,
                control,
            )?;
        }
        Kind::Xlsx => crate::discipline_xlsx::write_controlled(
            &request.path,
            prepared.datasets(),
            &request.rules,
            control,
        )?,
        Kind::Template => unreachable!("handled before object selection"),
    }
    Ok(Exported {
        skipped: prepared.skipped,
        companion: None,
        warnings: Vec::new(),
    })
}
impl Exporter {
    pub fn new() -> Result<Self, String> {
        Self::with_processor(process)
    }
    #[doc(hidden)]
    pub fn with_processor<F>(processor: F) -> Result<Self, String>
    where
        F: Fn(&Inputs, &Request, &Control) -> Result<Exported, String> + Send + 'static,
    {
        let (requests, rx) = mpsc::sync_channel::<Work>(1);
        let (tx, results) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("sleufbase-export".into())
            .spawn(move || {
                while let Ok(work) = rx.recv() {
                    let mut result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        work.control.check()?;
                        processor(&work.inputs, &work.request, &work.control)
                    }))
                    .unwrap_or_else(|_| {
                        Err(
                            "Exporttaak is onverwacht gestopt; projectgegevens zijn behouden"
                                .into(),
                        )
                    });
                    if work.control.is_cancelled() {
                        result = Err("Export geannuleerd".into());
                    }
                    work.control.finish();
                    if tx
                        .send(Ready {
                            inputs: work.inputs,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|e| format!("Exportworker kon niet starten: {e}"))?;
        Ok(Self {
            requests,
            results,
            active: None,
            closed: false,
        })
    }
    pub fn start(&mut self, inputs: Inputs, request: Request) -> Result<(), Box<StartFailure>> {
        if self.active.is_some() || self.closed {
            return Err(Box::new(StartFailure {
                inputs,
                message: "De exportworker is bezig of gestopt".into(),
            }));
        }
        let control = Arc::new(Control::default());
        let work = Work {
            inputs,
            request,
            control: control.clone(),
        };
        match self.requests.try_send(work) {
            Ok(()) => {
                self.active = Some(control);
                Ok(())
            }
            Err(TrySendError::Full(work)) => Err(Box::new(StartFailure {
                inputs: work.inputs,
                message: "De exportworker is bezig".into(),
            })),
            Err(TrySendError::Disconnected(work)) => {
                self.closed = true;
                Err(Box::new(StartFailure {
                    inputs: work.inputs,
                    message: "De exportworker is gestopt".into(),
                }))
            }
        }
    }
    pub fn control(&self) -> Option<&Control> {
        self.active.as_deref()
    }
    pub fn is_closed(&self) -> bool {
        self.closed
    }
    pub fn cancel(&self) -> bool {
        self.control().is_some_and(Control::cancel)
    }
    pub fn poll(&mut self) -> Option<Ready> {
        match self.results.try_recv() {
            Ok(ready) => {
                self.active = None;
                Some(ready)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.closed = true;
                None
            }
        }
    }
}
impl Drop for Exporter {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::{Duration, Instant},
    };
    fn inputs() -> Inputs {
        Inputs {
            datasets: vec![crate::parse_job_features(
                include_str!("../tests/fixtures/jobFeatures.json"),
                1,
                "PS1",
            )
            .unwrap()],
            ..Default::default()
        }
    }
    fn request(path: PathBuf, kind: Kind) -> Request {
        Request {
            path,
            kind,
            selection: Selection::all(1, 0),
            rules: crate::RULES.to_vec(),
            extra_layers: false,
            options: Default::default(),
            template: None,
            discipline_excel: false,
        }
    }
    fn wait(worker: &mut Exporter) -> Ready {
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(ready) = worker.poll() {
                return ready;
            }
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    #[test]
    fn real_exports_reuse_one_worker_and_return_the_same_owned_feature_allocation() {
        let dir = tempfile::tempdir().unwrap();
        let mut worker = Exporter::new().unwrap();
        let mut data = inputs();
        let pointer = data.datasets[0].features.as_ptr();
        let expected = serde_json::to_value(&data.datasets).unwrap();
        for kind in [Kind::Dxf, Kind::Xlsx] {
            let path = dir.path().join(if kind == Kind::Dxf {
                "objects.dxf"
            } else {
                "report.xlsx"
            });
            worker.start(data, request(path.clone(), kind)).unwrap();
            let ready = wait(&mut worker);
            assert!(ready.result.is_ok());
            assert!(path.exists());
            assert_eq!(ready.inputs.datasets[0].features.as_ptr(), pointer);
            assert_eq!(
                serde_json::to_value(&ready.inputs.datasets).unwrap(),
                expected
            );
            data = ready.inputs;
        }
    }
    #[test]
    fn partial_output_error_and_panic_return_inputs_and_do_not_poison_the_worker() {
        let dir = tempfile::tempdir().unwrap();
        let mut worker = Exporter::new().unwrap();
        let data = inputs();
        let pointer = data.datasets[0].features.as_ptr();
        let path = dir.path().join("partial.dxf");
        let mut req = request(path.clone(), Kind::Dxf);
        req.options.max_bytes = 32;
        worker.start(data, req).unwrap();
        let ready = wait(&mut worker);
        assert!(ready.result.is_err());
        assert_eq!(ready.inputs.datasets[0].features.as_ptr(), pointer);
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        let calls = AtomicUsize::new(0);
        let mut recovering = Exporter::with_processor(move |inputs, req, control| {
            if calls.fetch_add(1, Ordering::Relaxed) == 0 {
                panic!("controlled export failure")
            }
            process(inputs, req, control)
        })
        .unwrap();
        recovering
            .start(ready.inputs, request(path.clone(), Kind::Dxf))
            .unwrap();
        let ready = wait(&mut recovering);
        assert!(ready.result.unwrap_err().contains("projectgegevens"));
        assert!(!path.exists());
        recovering
            .start(ready.inputs, request(path.clone(), Kind::Dxf))
            .unwrap();
        let ready = wait(&mut recovering);
        assert!(ready.result.is_ok());
        assert_eq!(ready.inputs.datasets[0].features.as_ptr(), pointer);
        let before = std::fs::read(&path).unwrap();
        recovering
            .start(ready.inputs, request(path.clone(), Kind::Dxf))
            .unwrap();
        let ready = wait(&mut recovering);
        assert!(ready.result.is_err());
        assert_eq!(std::fs::read(path).unwrap(), before);
    }
    #[test]
    fn a_busy_worker_rejects_a_second_input_owner_and_cancellation_still_returns_the_first() {
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (gate_tx, gate_rx) = mpsc::sync_channel(1);
        let mut worker = Exporter::with_processor(move |inputs, req, control| {
            started_tx.send(()).unwrap();
            gate_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            process(inputs, req, control)
        })
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cancelled.dxf");
        let first = inputs();
        let pointer = first.datasets[0].features.as_ptr();
        worker
            .start(first, request(path.clone(), Kind::Dxf))
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let second = inputs();
        let second_pointer = second.datasets[0].features.as_ptr();
        let failure = worker
            .start(second, request(path.clone(), Kind::Dxf))
            .unwrap_err();
        assert_eq!(failure.inputs.datasets[0].features.as_ptr(), second_pointer);
        assert!(worker.cancel());
        gate_tx.send(()).unwrap();
        let ready = wait(&mut worker);
        assert!(ready.result.unwrap_err().contains("geannuleerd"));
        assert_eq!(ready.inputs.datasets[0].features.as_ptr(), pointer);
        assert!(!path.exists());
    }
}
