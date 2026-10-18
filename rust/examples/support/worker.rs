use sleufbase_rust::export_worker::{Exporter, Inputs, Request};
pub fn run(worker: &mut Exporter, inputs: Inputs, request: Request) -> Result<Inputs, String> {
    worker
        .start(inputs, request)
        .map_err(|failure| failure.message)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        if let Some(ready) = worker.poll() {
            ready.result?;
            return Ok(ready.inputs);
        }
        if std::time::Instant::now() >= deadline {
            return Err("Fixture export timed out".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
