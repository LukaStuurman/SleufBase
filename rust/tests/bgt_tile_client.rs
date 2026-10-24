use sleufbase_rust::{
    bgt_tile_client::{read_payload, Client, Options, Transport},
    bgt_tiles::{TileId, MAX_TILE_BYTES, ORIGIN_X, ORIGIN_Y, SPAN},
    Bounds,
};
use std::{
    io::{Read, Write},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
};
const SOURCE: &[u8] = include_bytes!("fixtures/bgt-tiles/value-types.pbf");
struct Mock {
    calls: Arc<Mutex<Vec<TileId>>>,
    bad: AtomicUsize,
    panic: AtomicBool,
    cancel: bool,
}
impl Transport for Mock {
    fn load(&self, id: TileId, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
        self.calls.lock().unwrap().push(id);
        if self.panic.swap(false, Ordering::Relaxed) {
            panic!("simulated tile transport panic");
        }
        if self.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        if self
            .bad
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_sub(1))
            .is_ok()
        {
            Ok(vec![0])
        } else {
            Ok(SOURCE.to_vec())
        }
    }
}
fn mock(bad: usize, panic: bool, cancel: bool) -> (Mock, Arc<Mutex<Vec<TileId>>>) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    (
        Mock {
            calls: calls.clone(),
            bad: AtomicUsize::new(bad),
            panic: AtomicBool::new(panic),
            cancel,
        },
        calls,
    )
}
fn options() -> Options {
    Options {
        backoff_millis: 0,
        cache_entries: 2,
        ..Options::default()
    }
}
fn bounds() -> Bounds {
    Bounds {
        min_x: ORIGIN_X + 2000. * SPAN + 10.,
        min_y: ORIGIN_Y - 2001. * SPAN + 10.,
        max_x: ORIGIN_X + 2003. * SPAN - 10.,
        max_y: ORIGIN_Y - 2000. * SPAN - 10.,
    }
}
#[test]
fn decoded_cache_shares_geometry_and_evicts_least_recently_used() {
    let (transport, calls) = mock(0, false, false);
    let client = Client::with_transport(transport, options()).unwrap();
    let cancel = AtomicBool::new(false);
    let a = TileId {
        row: 2000,
        col: 2000,
    };
    let b = TileId {
        row: 2000,
        col: 2001,
    };
    let c = TileId {
        row: 2000,
        col: 2002,
    };
    let first = client.tile(a, &cancel).unwrap();
    client.tile(b, &cancel).unwrap();
    assert!(Arc::ptr_eq(&first, &client.tile(a, &cancel).unwrap()));
    client.tile(c, &cancel).unwrap();
    client.tile(b, &cancel).unwrap();
    assert_eq!(*calls.lock().unwrap(), vec![a, b, c, b]);
}
#[test]
fn malformed_payload_retries_and_is_never_cached() {
    let (transport, calls) = mock(1, false, false);
    let client = Client::with_transport(transport, options()).unwrap();
    let cancel = AtomicBool::new(false);
    let id = TileId { row: 1, col: 2 };
    client.tile(id, &cancel).unwrap();
    client.tile(id, &cancel).unwrap();
    assert_eq!(calls.lock().unwrap().len(), 2);
    let (transport, calls) = mock(100, false, false);
    let client = Client::with_transport(
        transport,
        Options {
            retries: 2,
            ..options()
        },
    )
    .unwrap();
    assert!(client.tile(id, &cancel).is_err());
    assert!(client.tile(id, &cancel).is_err());
    assert_eq!(calls.lock().unwrap().len(), 4);
}
#[test]
fn parallel_batches_preserve_order_and_fail_without_partial_results() {
    let cancel = AtomicBool::new(false);
    let (transport, _) = mock(0, false, false);
    let client = Client::with_transport(transport, options()).unwrap();
    let actual = client.fetch(bounds(), &cancel).unwrap();
    assert_eq!(
        actual.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        vec![
            TileId {
                row: 2000,
                col: 2000
            },
            TileId {
                row: 2000,
                col: 2001
            },
            TileId {
                row: 2000,
                col: 2002
            }
        ]
    );
    let (transport, _) = mock(0, false, false);
    let client = Client::with_transport(
        transport,
        Options {
            result_bytes: 1,
            ..options()
        },
    )
    .unwrap();
    assert!(client.fetch(bounds(), &cancel).is_err());
    let (transport, _) = mock(0, true, false);
    let client = Client::with_transport(transport, options()).unwrap();
    assert!(client.fetch(bounds(), &cancel).is_err());
    assert_eq!(client.fetch(bounds(), &cancel).unwrap().len(), 3);
    let (transport, calls) = mock(0, false, true);
    let client = Client::with_transport(transport, options()).unwrap();
    assert!(client.fetch(bounds(), &cancel).is_err());
    assert!(calls.lock().unwrap().len() <= 3);
}
fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    e.write_all(bytes).unwrap();
    e.finish().unwrap()
}
#[test]
fn gzip_members_corruption_bombs_raw_limits_and_cancellation_are_bounded() {
    let cancel = AtomicBool::new(false);
    let mut members = gzip(b"first");
    members.extend(gzip(b"second"));
    assert_eq!(
        read_payload(members.as_slice(), "GZip", &cancel).unwrap(),
        b"firstsecond"
    );
    assert!(read_payload(&[0x1f, 0x8b, 0][..], "gzip", &cancel).is_err());
    assert!(read_payload(&[][..], "br", &cancel).is_err());
    assert!(read_payload(SOURCE, "identity", &AtomicBool::new(true)).is_err());
    assert!(read_payload(
        std::io::repeat(0).take(MAX_TILE_BYTES as u64 + 1),
        "identity",
        &cancel
    )
    .is_err());
    let bomb = gzip(&vec![0; MAX_TILE_BYTES + 1]);
    assert!(read_payload(bomb.as_slice(), "gzip", &cancel).is_err());
    let (transport, _) = mock(0, false, false);
    assert!(Client::with_transport(
        transport,
        Options {
            workers: 0,
            ..options()
        }
    )
    .is_err());
}
