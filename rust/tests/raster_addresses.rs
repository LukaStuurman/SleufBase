use serde_json::Value;
use sleufbase_rust::{
    location_search::ReverseLocation,
    raster_addresses::{self, Record, Source, Worker},
};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};
fn reference() -> Value {
    serde_json::from_str(include_str!("fixtures/raster-addresses/reference.json")).unwrap()
}
#[test]
fn original_row_format_best_match_errors_and_actual_copy_strings_match() {
    let data = reference();
    for c in data["cases"].as_array().unwrap() {
        let source: Source = serde_json::from_value(c["source"].clone()).unwrap();
        let result = if let Some(e) = c["error"].as_str() {
            Err(e.to_owned())
        } else {
            Ok(serde_json::from_value::<Vec<ReverseLocation>>(c["response"].clone()).unwrap())
        };
        let record = Record::from_result(source.name, result);
        assert_eq!(serde_json::to_value(&record).unwrap(), c["record"]);
        for copy in c["copies"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["selected"] == true)
        {
            assert_eq!(
                record.copied(copy["include_name"].as_bool().unwrap()),
                copy["clipboard"][0].as_str().unwrap()
            );
        }
    }
}
#[test]
fn complete_batch_preserves_source_order_failures_duplicates_and_progress() {
    let data = reference();
    let cases = data["cases"].as_array().unwrap();
    let sources = cases
        .iter()
        .map(|c| serde_json::from_value(c["source"].clone()).unwrap())
        .collect();
    let cancel = AtomicBool::new(false);
    let progress = AtomicUsize::new(0);
    let mut index = 0;
    let rows = raster_addresses::lookup(sources, &cancel, &progress, |x, y, _| {
        let c = &cases[index];
        index += 1;
        assert_eq!(
            [x, y],
            serde_json::from_value::<[f64; 2]>(c["source"]["center"].clone()).unwrap()
        );
        if let Some(e) = c["error"].as_str() {
            Err(e.to_owned())
        } else {
            Ok(serde_json::from_value(c["response"].clone()).unwrap())
        }
    })
    .unwrap();
    assert_eq!(progress.load(Ordering::Relaxed), cases.len());
    assert_eq!(index, cases.len());
    assert_eq!(
        serde_json::to_value(rows).unwrap(),
        Value::Array(cases.iter().map(|c| c["record"].clone()).collect())
    );
}
fn source(x: f64) -> Source {
    Source {
        name: "same.tif".into(),
        center: [x, 401000.],
        epsg: Some(28992),
    }
}
fn wait(worker: &Worker) -> raster_addresses::Ready {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(r) = worker.poll() {
            return r;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn replacement_cancels_a_whole_batch_and_never_publishes_its_partial_rows() {
    let (started, rx) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let worker = Worker::with_lookup(move |x, _, cancel| {
        started.send(x).unwrap();
        gate.recv_timeout(Duration::from_secs(5)).unwrap();
        if x == 1. {
            assert!(cancel.load(Ordering::Relaxed));
        }
        Ok(vec![ReverseLocation {
            label: x.to_string(),
            distance_m: 0.,
            location_type: "adres".into(),
        }])
    })
    .unwrap();
    worker.start(vec![source(1.), source(2.)]).unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), 1.);
    let ticket = worker.start(vec![source(3.)]).unwrap();
    release.send(()).unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), 3.);
    release.send(()).unwrap();
    let ready = wait(&worker);
    assert_eq!(ready.ticket, ticket.id);
    assert_eq!(ready.result.unwrap()[0].address, "3");
    assert_eq!(ticket.progress.load(Ordering::Relaxed), 1);
}
#[test]
fn invalid_projection_cancellation_and_size_limits_publish_no_invented_addresses() {
    let cancel = AtomicBool::new(false);
    let progress = AtomicUsize::new(0);
    let mut invalid = source(1.);
    invalid.epsg = Some(4326);
    let mut nan = source(f64::NAN);
    nan.name = "NaN.tif".into();
    let rows = raster_addresses::lookup(vec![invalid, nan], &cancel, &progress, |_, _, _| {
        panic!("Invalid input sent to provider")
    })
    .unwrap();
    assert!(rows
        .iter()
        .all(|r| r.address.starts_with("Adres ophalen mislukt:") && r.distance == "-"));
    assert!(raster_addresses::lookup(
        vec![source(1.); 5001],
        &cancel,
        &progress,
        |_, _, _| unreachable!()
    )
    .is_err());
    assert!(raster_addresses::lookup(
        vec![source(1.), source(2.)],
        &cancel,
        &progress,
        |_, _, cancel| {
            cancel.store(true, Ordering::Relaxed);
            Ok(Vec::new())
        }
    )
    .is_err());
    cancel.store(false, Ordering::Relaxed);
    assert!(
        raster_addresses::lookup(vec![source(1.); 3], &cancel, &progress, |_, _, _| Ok(vec![
            ReverseLocation {
                label: "a".repeat(2 * 1024 * 1024),
                distance_m: 1.,
                location_type: "adres".into()
            }
        ]))
        .is_err()
    );
}
