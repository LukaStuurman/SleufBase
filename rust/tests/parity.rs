use sleufbase_rust::{
    parse_job_features, summarize, write_dxf, write_xlsx, Bounds, Geometry, RULES,
};

fn example() -> sleufbase_rust::Dataset {
    let text = include_str!("fixtures/jobFeatures.json");
    parse_job_features(text, 123, "PS123").unwrap()
}

#[test]
fn kickthemap_objects_and_disciplines_parity() {
    let job = example();
    assert_eq!(job.features.len(), 4);
    assert!(matches!(job.features[2].geometry, Geometry::Polyline(_)));
    let summary = summarize(&job, RULES);
    assert_eq!(summary.distinct_count(), 3);
    assert_eq!(summary.total_count(), 4);
    assert_eq!(summary.counts[0], 2);
    let bounds = Bounds::from_datasets(&[job]).unwrap();
    assert_eq!(bounds.min_x, 168000.0);
    assert_eq!(bounds.max_y, 401004.0);
}

#[test]
fn excel_and_dxf_are_written_with_correct_formats() {
    let path = std::env::temp_dir().join(format!(
        "sleufbase-rust-parity-{}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    let spreadsheet = path.join("parity.xlsx");
    let dxf = path.join("parity.dxf");
    let _ = std::fs::remove_file(&spreadsheet);
    let _ = std::fs::remove_file(&dxf);
    write_xlsx(&spreadsheet, &[example()], RULES).unwrap();
    write_dxf(&dxf, &[example()], RULES).unwrap();
    assert_eq!(&std::fs::read(&spreadsheet).unwrap()[..2], b"PK");
    assert!(std::fs::read_to_string(&dxf).unwrap().contains("0\nLINE\n"));
    std::fs::remove_file(spreadsheet).unwrap();
    std::fs::remove_file(dxf).unwrap();
}