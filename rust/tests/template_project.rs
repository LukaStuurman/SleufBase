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
        font_directories:Vec::new(),
        paired:false,
        template:fixture.template.clone(),
        project:"Native projectport".into(), drawn_by:"Techbase".into(),
        background:BackgroundKind::Blank, dpi:96, scale:225.,
        include_bgt:false, include_cadastral:false, include_cad:false,
        choices:vec![Choice {
            source:Source::Job(0),raster:Some(0),include:true,
            title:"Automatisch vak 1".into(),profile:false,
            reverse:false,comments:"Geïmporteerde projectgegevens".into(),photos:Vec::new(),
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
    let tiff_bounds=tiff.bounds;
    let trench=sleufbase_rust::virtual_trench::VirtualTrench::new(
        "PS-virtual".into(),start,end,0.6).unwrap();
    let opts=Options {
        paired:false,
        template:fixture.template,
        choices:vec![Choice {source:Source::Virtual(0),raster:Some(0),
            include:true,title:"Virtueel profiel".into(),profile:true,
            reverse:false,comments:String::new(),photos:Vec::new()}],
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
    let overlays=&slot.map_source.as_ref().unwrap().overlays;
    assert_eq!(overlays.len(),4, "four clipped trench outline edges");
    assert!(overlays.iter().all(|stroke|stroke.points.len()==2 &&
        stroke.points.iter().all(|p|p[0]>=tiff_bounds.min_x && p[0]<=tiff_bounds.max_x &&
            p[1]>=tiff_bounds.min_y && p[1]<=tiff_bounds.max_y)));
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
        paired:false,
        template: source.template.clone(),
        dpi:96,scale:225.,
        include_bgt:false,include_cadastral:false,include_cad:true,
        background:BackgroundKind::Blank,
        choices:vec![Choice {
            source:Source::Job(0),raster:Some(0),include:true,
            title:"CAD op kaart".into(),profile:false,reverse:false,
            comments:String::new(),photos:Vec::new(),
        }],
        ..Options::default()
    };
    let request=template_project::build_with_cad(
        &options,&source.datasets,&[tiff],&[],&[],None,&[cad],
        &source.rules,&Control::default(),
    ).unwrap();
    let overlays=&request.slots[0].as_ref().unwrap().map_source.as_ref().unwrap().overlays;
    assert!(overlays.iter().any(|stroke| {
        stroke.points.len()==2
            && (stroke.points[0][0]-bounds.min_x).abs()<1e-6
            && (stroke.points[1][0]-bounds.max_x).abs()<1e-6
            && (stroke.points[0][1]-center_y).abs()<1e-6
            && (stroke.points[1][1]-center_y).abs()<1e-6
    }),"a CAD line crossing the viewport must not disappear");
    let temp=tempfile::tempdir().unwrap();
    let output=temp.path().join("project-cad.dxf");
    let result=template_export::export(&output,&request,&Control::default()).unwrap();
    assert_eq!(result.filled_slots,1);
    assert!(output.exists());
    assert!(result.published.assets.join("slot00001_kaart.png").is_file());
    assert!(result.published.assets.join("slot00001_tiff.png").is_file());
}

#[test]
fn template_photos_are_persistently_bundled_without_replacing_tiff_or_map() {
    use sleufbase_rust::template_photo::{Photo,Area};
    let (reference,tiff)=fixture();
    let dir=tempfile::tempdir().unwrap();
    let photo=dir.path().join("site.png");
    let mut pixels=image::RgbaImage::from_pixel(64,48,image::Rgba([36,64,112,255]));
    pixels.put_pixel(0,0,image::Rgba([255,10,20,255]));
    pixels.save(&photo).unwrap();
    let options=Options {
        paired:false,
        template:reference.template,
        include_bgt:false,include_cadastral:false,include_cad:false,
        choices:vec![Choice {
            source:Source::Job(0),raster:Some(0),include:true,
            title:"Vak met foto's".into(),profile:false,reverse:false,
            comments:String::new(),photos:vec![
                Photo{path:photo.clone(),turns:1,area:Area::Tiff},
                Photo{path:photo.clone(),turns:0,area:Area::Map},
            ],
        }],
        ..Options::default()
    };
    let req=template_project::build(&options,&reference.datasets,&[tiff],
        &[],&[],None,&reference.rules,&Control::default()).unwrap();
    assert_eq!(req.slots[0].as_ref().unwrap().photos.len(),2);
    let output=dir.path().join("photos.dxf");
    let result=template_export::export(&output,&req,&Control::default()).unwrap();
    for asset in [
        "slot00001_tiff.png","slot00001_kaart.png",
        "slot00001_photo01.png","slot00001_photo02.png"
    ] { assert!(result.published.assets.join(asset).is_file(),"{asset}"); }
    let rotated=image::open(result.published.assets.join("slot00001_photo01.png")).unwrap();
    assert_eq!((rotated.width(),rotated.height()),(48,64));
    let doc=sleufbase_rust::dxf_document::Document::load(
        &output,&std::sync::atomic::AtomicBool::new(false)).unwrap();
    assert_eq!(doc.records().filter(|r|r.kind()=="IMAGEDEF").count(),4);
    assert_eq!(doc.records().filter(|r|r.kind()=="IMAGE").count(),4);
    // Input source remains unchanged and is never relocated/deleted.
    assert_eq!(image::open(&photo).unwrap().width(),64);
}

#[test]
fn virtual_marxact_sheet_exports_full_png_and_profile_source_without_external_geotiff() {
    use sleufbase_rust::virtual_trench::{VirtualTrench,TrenchPoint};
    use sleufbase_rust::model::Vertex;
    let (reference,_)=fixture();
    let mut trench=VirtualTrench::new(
        "Zelfstandig virtueel raster".into(),
        Vertex{x:155000.,y:463000.,z:Some(-1.0)},
        Vertex{x:155008.,y:463000.,z:Some(-1.2)},0.6,
    ).unwrap();
    trench.objects.push(TrenchPoint{
        role:"water".into(),object_name:"Waterleiding".into(),
        source_name:"WATER".into(),
        position:Vertex{x:155003.,y:463000.,z:Some(-1.5)},
        attributes:Default::default(),
    });
    let dir=tempfile::tempdir().unwrap();
    let options=Options {
        paired:false,
        template:reference.template,
        include_bgt:false,include_cadastral:false,include_cad:false,
        background:BackgroundKind::Blank,
        choices:vec![Choice{
            source:Source::Virtual(0),raster:None,include:true,
            title:trench.label.clone(),profile:false,reverse:false,
            comments:"Eigen meetraster zonder KickTheMap".into(),photos:Vec::new(),
        }],
        ..Options::default()
    };
    let request=template_project::build(
        &options,&[],&[],&[trench.clone()],&[],None,&reference.rules,
        &Control::default(),
    ).unwrap();
    let slot=request.slots[0].as_ref().unwrap();
    assert!(slot.tiff_png.as_os_str().is_empty());
    assert!(slot.tiff_source.is_none());
    assert!(slot.virtual_source.is_some());
    assert_eq!(request.datasets[0].features.len(),3);
    let expected=slot.bounds;
    assert!(expected.min_x < trench.start.x);
    assert!(expected.max_x > trench.end.x);
    let output=dir.path().join("standalone-virtual.dxf");
    let export=template_export::export(&output,&request,&Control::default()).unwrap();
    assert_eq!(export.filled_slots,1);
    let photo=export.published.assets.join("slot00001_tiff.png");
    assert!(photo.is_file());
    let synthetic=image::open(photo).unwrap().to_rgba8();
    assert!(synthetic.pixels().any(|p|p[3]==0));
    assert!(synthetic.pixels().any(|p|p[0]==0&&p[3]==255));
    assert!(synthetic.pixels().any(|p|p[2]==229&&p[3]>=235));
    assert!(export.published.assets.join("slot00001_kaart.png").is_file());
    let doc=sleufbase_rust::dxf_document::Document::load(
        &output,&std::sync::atomic::AtomicBool::new(false)).unwrap();
    assert!(doc.records().filter(|x|x.kind()=="IMAGEDEF").count()>=2);
    assert_eq!(request.datasets[0].features[1].object_name,"Waterleiding");

    let mut with_profile=options;
    with_profile.choices[0].profile=true;
    let project=template_project::build(&with_profile,&[],&[],&[trench],
        &[],None,&reference.rules,&Control::default()).unwrap();
    assert!(project.slots[0].as_ref().unwrap().profile.is_some());
    assert_eq!(project.slots[0].as_ref().unwrap().profile.as_ref().unwrap().forced_start,
        Some([155000.,463000.]));
}
#[test]
fn real_job_still_requires_geotiff_but_virtual_job_does_not(){
    let (reference,_)=fixture();
    let opts=Options{
        template:reference.template,
        choices:vec![Choice{source:Source::Job(0),raster:None,include:true,
            title:"Echte KickTheMap-job".into(),profile:false,reverse:false,
            comments:String::new(),photos:Vec::new()}],
        ..Options::default()
    };
    let err=template_project::build(&opts,&reference.datasets,&[],&[],&[],None,
        &reference.rules,&Control::default()).err().unwrap();
    assert!(err.contains("GeoTIFF"));
}
