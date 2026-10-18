use sleufbase_rust::{cross_section, dxf_read, legacy_project, marxact, parse, project};
use std::{path::Path, sync::atomic::AtomicBool};

#[test]
fn pre_cancelled_imports_never_open_or_return_partial_content() {
    let cancel = AtomicBool::new(true);
    let missing = Path::new("does-not-exist");
    assert!(project::read_project_with_cancel(missing, &cancel)
        .unwrap_err()
        .contains("geannuleerd"));
    assert!(
        legacy_project::read_legacy_project_with_cancel(missing, &cancel)
            .unwrap_err()
            .contains("geannuleerd")
    );
    assert!(dxf_read::load_dxf_with_cancel(missing, &cancel)
        .unwrap_err()
        .contains("geannuleerd"));
    assert!(parse::load_job_features_with_cancel(missing, &cancel)
        .unwrap_err()
        .contains("geannuleerd"));
    assert!(
        marxact::import_dxf_with_cancel(missing, &Default::default(), &cancel)
            .unwrap_err()
            .contains("geannuleerd")
    );
    assert!(
        dxf_read::parse_dxf_bytes_with_cancel(b"broken", missing, &cancel)
            .unwrap_err()
            .contains("geannuleerd")
    );
    let p = sleufbase_rust::Vertex {
        x: 0.,
        y: 0.,
        z: None,
    };
    let q = sleufbase_rust::Vertex { x: 10., ..p };
    let trench = sleufbase_rust::virtual_trench::VirtualTrench::new("T".into(), p, q, 1.).unwrap();
    assert!(cross_section::markers_with_cancel(&trench, &cancel)
        .unwrap_err()
        .contains("geannuleerd"));
}
