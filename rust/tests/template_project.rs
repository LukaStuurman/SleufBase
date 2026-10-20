use sleufbase_rust::{
    export_control::Control,
    geotiff::GeoTiffInfo,
    map_export::Background,
    template_export,
    template_project::{self, BackgroundKind, Choice, Options, Source},
};
use std::path::PathBuf;

fn fixture() -> (template_export::Request, GeoTiffInfo) {
    let data: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/template-export/reference.json"
    )).unwrap();
    let value = data.as_array().unwrap().iter()
        .find(|v|v["name"] == "native-tiff").unwrap()["request"].clone();
    let mut prepared:template_export::Request = serde_json::from_value(value).unwrap();
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    prepared.template = base.join(&prepared.template);
    let input = prepared.slots.iter().flatten()
        .find_map(|s|s.tiff_source.as_ref().map(|source|(source, s.bounds)))
        .unwrap();
    let geotiff = GeoTiffInfo {
        path: base.join(&input.0.path), width: 100, height: 100,
        transform: input.0.transform, bounds: input.1, epsg:Some(28992),
        kickthemap_job_id:Some(9),kickthemap_job_title:String::new(),
        kickthemap_job_sidecar:None,
    };
    (prepared,geotiff)
}
#[test]
fn real_template_pipeline_can_export_from_a_loaded_project_without_manifest_or_network() {
    let (fixture,tiff) = fixture();
    assert!(!fixture.datasets.is_empty());
    let dir=tempfile::tempdir().unwrap();
    let mut options=Options {
        template:fixture.template.clone(),
        project:"Native projectport".into(), drawn_by:"Techbase".into(),
        background:BackgroundKind::Blank, dpi:96, scale:225.,
        include_bgt:false, include_cadastral:false, include_cad:false,
        choices:vec![Choice {
            source:Source::Job(0),raster:Some(0),include:true,
            title:"Automatisch vak 1".into(),profile:false,
            reverse:false,comments:"Geïmporteerde projectgegevens".into(),
        }],
    };
    let output=dir.path().join("project-output.dxf");
    let request=template_project::build(&options,&fixture.datasets,&[tiff.clone()],
        &[],&[],None,&fixture.rules,&Control::default()).unwrap();
    assert_eq!(request.slots.len(),1);
    assert_eq!(request.datasets.len(),1);
    assert_eq!(request.slots[0].as_ref().unwrap().title,"Automatisch vak 1");
    assert!(matches!(request.slots[0].as_ref().unwrap().map_source.as_ref().unwrap().background,
        Background::Solid{..}));
    let result=template_export::export(&output,&request,&Control::default()).unwrap();
    assert_eq!(result.filled_slots,1);
    assert!(output.is_file());
    assert!(result.published.assets.join("slot00001_tiff.png").is_file());
    assert!(result.published.assets.join("slot00001_kaart.png").is_file());
    let doc=sleufbase_rust::dxf_document::Document::load(
        &output,&std::sync::atomic::AtomicBool::new(false)).unwrap();
    assert!(doc.records().any(|r|r.kind()=="IMAGEDEF"));
    // Same input can safely be exported again under a distinct output name.
    options.choices[0].title="Tweede vak".into();
    let second=template_project::build(&options,&fixture.datasets,&[tiff],
        &[],&[],None,&fixture.rules,&Control::default()).unwrap();
    assert_eq!(second.slots[0].as_ref().unwrap().title,"Tweede vak");
}
#[test]
fn virtual_trench_keeps_measured_start_and_selected_source() {
    let (fixture,tiff)=fixture();
    let x = (tiff.bounds.min_x + tiff.bounds.max_x) / 2.0;
    let y = (tiff.bounds.min_y + tiff.bounds.max_y) / 2.0;
    let half_width = (tiff.bounds.max_x - tiff.bounds.min_x) / 4.0;
    let start=sleufbase_rust::Vertex{x:x-half_width,y,z:Some(-1.)};
    let end=sleufbase_rust::Vertex{x:x+half_width,y,z:Some(-1.)};
    let trench=sleufbase_rust::virtual_trench::VirtualTrench::new(
        "PS-virtual".into(),start,end,0.6).unwrap();
    let opts=Options {
        template:fixture.template,
        choices:vec![Choice {source:Source::Virtual(0),raster:Some(0),
            include:true,title:"Virtueel profiel".into(),profile:true,
            reverse:false,comments:String::new()}],
        include_bgt:false,include_cadastral:false,
        ..Options::default()
    };
    let request=template_project::build(&opts,&[],&[tiff],&[trench],&[],None,
        &fixture.rules,&Control::default()).unwrap();
    assert_eq!(request.slots.len(),1);
    assert_eq!(request.datasets.len(),1);
    let slot=request.slots[0].as_ref().unwrap();
    assert_eq!(slot.profile.as_ref().unwrap().forced_start,Some([start.x,start.y]));
    assert_eq!(slot.tiff_source.as_ref().unwrap().axis,Some([
        [start.x,start.y],[end.x,end.y]
    ]));
    assert_eq!(slot.map_source.as_ref().unwrap().overlays.len(),1);
}

#[test]
fn exported_project_map_includes_clipped_visible_cad_lines_and_permanent_rasters() {
    use sleufbase_rust::{
        dxf_read::{DxfFeature, DxfOverlay},
        model::Vertex,
    };
    let (source, tiff) = fixture();
    let bounds = tiff.bounds;
    let center_y = (bounds.min_y + bounds.max_y) * 0.5;
    let mut cad = DxfOverlay::default();
    cad.visible = true;
    cad.features.push(DxfFeature {
        entity_type: "LINE".into(), handle: "A12".into(),
        layer: "Leidingen".into(), color_aci: 1,
        points: vec![
            Vertex {x:bounds.min_x-100.,y:center_y,z:None},
            Vertex {x:bounds.max_x+100.,y:center_y,z:None},
        ],
        raw_text:String::new(),block_name:String::new(),closed:None,cad:None,
    });
    let options=Options {
        template: source.template.clone(),
        dpi:96,scale:225.,
        include_bgt:false,include_cadastral:false,include_cad:true,
        background:BackgroundKind::Blank,
        choices:vec![Choice {
            source:Source::Job(0),raster:Some(0),include:true,
            title:"CAD op kaart".into(),profile:false,reverse:false,
            comments:String::new(),
        }],
        ..Options::default()
    };
    let request=template_project::build_with_cad(
        &options,&source.datasets,&[tiff],&[],&[],None,&[cad],
        &source.rules,&Control::default(),
    ).unwrap();
    let overlays=&request.slots[0].as_ref().unwrap().map_source.as_ref().unwrap().overlays;
    assert!(overlays.iter().any(|v|
        v.points==vec![[bounds.min_x,center_y],[bounds.max_x,center_y]]
    ),"a CAD line crossing the viewport must not disappear");
    let temp=tempfile::tempdir().unwrap();
    let output=temp.path().join("project-cad.dxf");
    let result=template_export::export(&output,&request,&Control::default()).unwrap();
    assert_eq!(result.filled_slots,1);
    assert!(output.exists());
    assert!(result.published.assets.join("slot00001_kaart.png").is_file());
    assert!(result.published.assets.join("slot00001_tiff.png").is_file());
}
