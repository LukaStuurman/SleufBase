//! Native Rust desktop UI, no Python runtime or IPC dependency.
//! Native project format is deliberately separate from legacy Python projects.

use eframe::egui::{self, Color32, Pos2, Stroke};
use sleufbase_rust::{
    load_job_features, write_dxf, write_xlsx, Bounds, Dataset, Geometry,
    Vertex, RULES
};
use sleufbase_rust::dxf_read::{load_dxf, DxfOverlay};
use sleufbase_rust::geotiff::{read_geotiff_metadata, GeoTiffInfo};
use sleufbase_rust::loaded_jobs::{kickthemap_cache_dir, read_active_loaded_job_ids};
use sleufbase_rust::project::{
    self, AutosaveSettings, NativeProject, read_project, save_project
};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const ORANGE: Color32 = Color32::from_rgb(249, 115, 22);
const DARK: Color32 = Color32::from_rgb(194, 65, 12);
const INK: Color32 = Color32::from_rgb(17, 24, 39);

pub struct SleufBaseApp {
    datasets: Vec<Dataset>,
    geotiffs: Vec<GeoTiffInfo>,
    raster_textures: Vec<Option<egui::TextureHandle>>,
    overlays: Vec<DxfOverlay>,
    active_only: bool,
    selected: Option<usize>,
    query: String,
    message: String,
    project_path: Option<PathBuf>,
    project_name: String,
    dirty: bool,
    backup_settings: AutosaveSettings,
    last_backup: Instant,
    zoom: f64,
    pan: egui::Vec2,
}

impl Default for SleufBaseApp {
    fn default() -> Self {
        Self {
            datasets:Vec::new(),
            geotiffs:Vec::new(),
            raster_textures:Vec::new(),
            overlays:Vec::new(),
            active_only:false,
            selected:None,
            query:String::new(),
            message:String::new(),
            project_path:None,
            project_name:"Naamloos project".into(),
            dirty:false,
            backup_settings:project::read_autosave_settings(
                &project::app_data_directory().join("autosave-settings.json")),
            last_backup:Instant::now(),
            zoom:1.0,
            pan:egui::Vec2::ZERO,
        }
    }
}

impl SleufBaseApp {
    fn snapshot(&self) -> NativeProject {
        NativeProject {
            name:self.project_name.clone(),
            datasets:self.datasets.clone(),
            rasters:self.geotiffs.clone(),
            overlays:self.overlays.clone(),
            ..NativeProject::default()
        }
    }

    fn new_project(&mut self) {
        self.datasets.clear();
        self.geotiffs.clear();
        self.raster_textures.clear();
        self.overlays.clear();
        self.selected=None;
        self.project_path=None;
        self.project_name="Naamloos project".into();
        self.dirty=false;
        self.zoom=1.0;
        self.pan=egui::Vec2::ZERO;
        self.message="Nieuw leeg Rust-project. De vorige gegevens zijn uit het huidige venster verwijderd.".into();
    }

    fn load_project_file(&mut self, path: PathBuf, ctx: &egui::Context) {
        match read_project(&path) {
            Ok(doc) => {
                let textures=doc.rasters.iter().map(|r|load_texture(&r.path,ctx)).collect();
                self.datasets=doc.datasets;
                self.geotiffs=doc.rasters;
                self.raster_textures=textures;
                self.overlays=doc.overlays;
                self.project_name=doc.name;
                self.selected=(!self.datasets.is_empty()).then_some(0);
                self.project_path=Some(path.clone());
                self.zoom=1.0;
                self.pan=egui::Vec2::ZERO;
                self.last_backup=Instant::now();
                self.dirty=false;
                self.message=format!("Rust-project geopend: {}",path.display());
            }
            Err(err) => self.message=format!("Project niet geladen (origineel ongewijzigd): {err}"),
        }
    }

    fn save_project_file(&mut self, save_as: bool) {
        let path = if !save_as {
            self.project_path.clone()
        } else { None };
        let path=path.or_else(||rfd::FileDialog::new()
            .add_filter("SleufBase Rust Project", &["json"])
            .set_file_name("Project.sleufbase-rust.json")
            .save_file());
        if let Some(path)=path {
            match save_project(&path,&self.snapshot()) {
                Ok(())=>{
                    self.project_path=Some(path.clone());
                    self.dirty=false;
                    self.message=format!("Rust-project opgeslagen: {}",path.display());
                }
                Err(err)=>self.message=format!("Project opslaan mislukt: {err}"),
            }
        }
    }

    fn try_autosave(&mut self) {
        if !self.backup_settings.enabled ||
            self.last_backup.elapsed() <
                Duration::from_secs(u64::from(self.backup_settings.interval_minutes)*60)
        {
            return;
        }
        // Reset first even on error to avoid repeatedly writing each repaint.
        self.last_backup=Instant::now();
        if !self.dirty {return;}
        match project::create_backup(
            &self.snapshot(),
            &project::backup_directory(),
            self.backup_settings.max_backups
        ) {
            Ok(path)=>self.message=format!("Automatische Rust-back-up: {}",path.display()),
            Err(err)=>self.message=format!("Automatische back-up mislukt: {err}"),
        }
    }

    fn load_files(&mut self, paths:Vec<PathBuf>) {
        let mut added=0;
        let mut failures=Vec::new();
        for path in paths {
            if self.datasets.iter().any(|ds| ds.source_path==path) {continue;}
            match load_job_features(&path) {
                Ok(dataset)=>{
                    self.datasets.push(dataset);
                    self.selected=Some(self.datasets.len()-1);
                    added+=1;
                }
                Err(err)=>failures.push(format!("{}: {err}",path.display()))
            }
        }
        if added>0 {self.dirty=true;}
        self.message = format!("{added} KickTheMap-job(s) geladen. {}",failures.join("; "));
    }

    fn load_geotiffs(&mut self, paths:Vec<PathBuf>, ctx:&egui::Context) {
        let mut added=0;
        let mut warnings=Vec::new();
        for path in paths {
            if self.geotiffs.iter().any(|info|info.path==path) {continue;}
            match read_geotiff_metadata(&path) {
                Ok(info)=>{
                    let texture=load_texture(&path,ctx);
                    if texture.is_none() {
                        warnings.push(format!("{}: geen 8-bit rasterpreview",path.display()));
                    }
                    self.geotiffs.push(info);
                    self.raster_textures.push(texture);
                    added+=1;
                }
                Err(err)=>warnings.push(format!("{}: {err}",path.display()))
            }
        }
        if added>0 {self.dirty=true;}
        self.message=format!("{added} GeoTIFF-bestanden geladen. {}",warnings.join("; "));
    }

    fn load_overlays(&mut self, paths:Vec<PathBuf>) {
        let mut added=0;
        let mut failures=Vec::new();
        for path in paths {
            if self.overlays.iter().any(|overlay|overlay.path==path) {continue;}
            match load_dxf(&path) {
                Ok(overlay)=>{self.overlays.push(overlay);added+=1;}
                Err(err)=>failures.push(format!("{}: {err}",path.display()))
            }
        }
        if added>0 {self.dirty=true;}
        self.message=format!("{added} DXF-overlays geladen. {}",failures.join("; "));
    }

    fn active_job_ids(&self) -> std::collections::BTreeSet<u64> {
        let now=SystemTime::now().duration_since(UNIX_EPOCH)
            .map(|d|d.as_secs_f64()).unwrap_or(0.0);
        read_active_loaded_job_ids(
            &kickthemap_cache_dir().join("active_loaded_jobs.json"),now
        )
    }

    fn save_excel(&mut self) {
        if self.datasets.is_empty() {
            self.message="Laad eerst jobFeatures.json".into();
            return;
        }
        if let Some(path)=rfd::FileDialog::new()
            .add_filter("Excel",&["xlsx"])
            .set_file_name("SleufBase-Disciplines-Rust.xlsx").save_file()
        {
            self.message=match write_xlsx(&path,&self.datasets,RULES) {
                Ok(())=>format!("Excel opgeslagen: {}",path.display()),
                Err(err)=>format!("Excel-export mislukt: {err}"),
            };
        }
    }

    fn save_dxf(&mut self) {
        if self.datasets.is_empty() {
            self.message="Laad eerst jobFeatures.json".into();
            return;
        }
        if let Some(path)=rfd::FileDialog::new()
            .add_filter("DXF",&["dxf"])
            .set_file_name("SleufBase-Objecten-Rust.dxf").save_file()
        {
            self.message=match write_dxf(&path,&self.datasets,RULES) {
                Ok(())=>format!("DXF opgeslagen: {}",path.display()),
                Err(err)=>format!("DXF-export mislukt: {err}"),
            };
        }
    }
}

fn load_texture(path:&std::path::Path,ctx:&egui::Context) -> Option<egui::TextureHandle> {
    let Ok((w,h,rgba))=sleufbase_rust::raster::decode_tiff_preview(path) else{return None;};
    let image=egui::ColorImage::from_rgba_unmultiplied([w as usize,h as usize],&rgba);
    Some(ctx.load_texture(
        path.to_string_lossy(),image,egui::TextureOptions::LINEAR
    ))
}

fn union_bounds(a:Option<Bounds>,b:Option<Bounds>)->Option<Bounds> {
    match (a,b) {
        (None,other)|(other,None)=>other,
        (Some(x),Some(y))=>Some(Bounds {
            min_x:x.min_x.min(y.min_x),min_y:x.min_y.min(y.min_y),
            max_x:x.max_x.max(y.max_x),max_y:x.max_y.max(y.max_y),
        }),
    }
}

fn aci_color(color:i32)->Color32 {
    match color.abs() {
        1=>Color32::from_rgb(220,45,45),
        2=>Color32::from_rgb(225,190,20),
        3=>Color32::from_rgb(40,150,40),
        4=>Color32::from_rgb(40,190,195),
        5=>Color32::from_rgb(45,100,220),
        6=>Color32::from_rgb(190,70,200),
        7=>Color32::from_rgb(45,45,45),
        _=>Color32::from_rgb(175,70,40),
    }
}

fn canvas(
    ui:&mut egui::Ui,
    datasets:&[Dataset],
    rasters:&[GeoTiffInfo],
    textures:&[Option<egui::TextureHandle>],
    overlays:&[DxfOverlay],
    zoom:&mut f64,
    pan:&mut egui::Vec2
) {
    let desired=ui.available_size();
    let (rect,response)=ui.allocate_exact_size(desired,egui::Sense::click_and_drag());
    let painter=ui.painter_at(rect);
    painter.rect_filled(rect,5.0,Color32::from_rgb(250,251,253));

    let mut bounds=Bounds::from_datasets(datasets);
    for raster in rasters {
        bounds=union_bounds(bounds,Some(raster.bounds));
    }
    for overlay in overlays.iter().filter(|o|o.visible) {
        bounds=union_bounds(bounds,overlay.bounds());
    }
    let Some(bounds)=bounds else {
        painter.text(rect.center(),egui::Align2::CENTER_CENTER,
            "Open een lokaal KickTheMap JSON-, GeoTIFF- of DXF-bestand",
            egui::FontId::proportional(17.0),INK);
        return;
    };

    if response.hovered() {
        let scroll=ui.input(|i|i.smooth_scroll_delta.y);
        if scroll!=0.0 {
            *zoom=(*zoom*(1.0+f64::from(scroll)*0.002).clamp(0.25,4.0))
                .clamp(0.05,300.0);
        }
    }
    if response.dragged() {*pan+=response.drag_delta();}

    let width=(bounds.max_x-bounds.min_x).max(1.0);
    let height=(bounds.max_y-bounds.min_y).max(1.0);
    let screen_width=(rect.width()-56.0).max(1.0) as f64;
    let screen_height=(rect.height()-56.0).max(1.0) as f64;
    let scale=(screen_width/width).min(screen_height/height) * *zoom;
    let cx=(bounds.min_x+bounds.max_x)*0.5;
    let cy=(bounds.min_y+bounds.max_y)*0.5;
    let center=rect.center()+*pan;
    let project=|x:f64,y:f64|Pos2::new(
        center.x+((x-cx)*scale) as f32,
        center.y-((y-cy)*scale) as f32
    );
    painter.add(egui::Shape::rect_filled(rect,0.0,Color32::TRANSPARENT));
    for (raster,texture) in rasters.iter().zip(textures.iter()) {
        if let Some(texture)=texture {
            let corners=[
                (0.0,0.0,0.0f32,0.0f32),
                (f64::from(raster.width),0.0,1.0,0.0),
                (f64::from(raster.width),f64::from(raster.height),1.0,1.0),
                (0.0,f64::from(raster.height),0.0,1.0)
            ];
            let mut mesh=egui::Mesh::with_texture(texture.id());
            for (col,row,u,v) in corners {
                let (x,y)=raster.transform.pixel_to_world(col,row);
                mesh.vertices.push(egui::epaint::Vertex {
                    pos:project(x,y),uv:Pos2::new(u,v),color:Color32::WHITE,
                });
            }
            mesh.indices.extend_from_slice(&[0,1,2,0,2,3]);
            painter.add(egui::Shape::mesh(mesh));
        }
    }
    for overlay in overlays.iter().filter(|o|o.visible) {
        for feature in &overlay.features {
            let stroke=Stroke::new(1.6,aci_color(feature.color_aci));
            for pair in feature.points.windows(2) {
                painter.line_segment([
                    project(pair[0].x,pair[0].y),
                    project(pair[1].x,pair[1].y)
                ],stroke);
            }
        }
    }
    for dataset in datasets {
        for feature in &dataset.features {
            let color=match sleufbase_rust::matching_rule(&feature.source_name,RULES) {
                Some(rule) if rule.label=="Waterleiding"=>Color32::from_rgb(40,105,235),
                Some(rule) if rule.label=="Laagspanning"=>Color32::from_rgb(236,174,20),
                Some(rule) if rule.label=="Middenspanning"=>Color32::from_rgb(235,40,45),
                Some(_)=>ORANGE,
                None=>Color32::GRAY,
            };
            match &feature.geometry {
                Geometry::Point(p)=>{painter.circle_filled(project(p.x,p.y),3.5,color);}
                Geometry::Polyline(vertices)=>{
                    for pair in vertices.windows(2) {
                        painter.line_segment([
                            project(pair[0].x,pair[0].y),
                            project(pair[1].x,pair[1].y)
                        ],Stroke::new(1.8,color));
                    }
                }
            }
        }
    }
    if let Some(pos)=response.hover_pos() {
        let x=cx+f64::from(pos.x-center.x)/scale;
        let y=cy-f64::from(pos.y-center.y)/scale;
        painter.text(pos+egui::vec2(12.0,8.0),egui::Align2::LEFT_TOP,
            format!("X {:.3} / Y {:.3}",x,y),
            egui::FontId::monospace(12.0),INK);
    }
    painter.text(rect.left_bottom()+egui::vec2(12.0,-12.0),
        egui::Align2::LEFT_BOTTOM,
        format!("X: {:.2} - {:.2}, Y: {:.2} - {:.2}; zoom {:.1}x",
            bounds.min_x,bounds.max_x,bounds.min_y,bounds.max_y,zoom),
        egui::FontId::monospace(11.0),Color32::DARK_GRAY);
}

impl eframe::App for SleufBaseApp {
    fn update(&mut self,ctx:&egui::Context,_frame:&mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::light());
        self.try_autosave();
        ctx.request_repaint_after(Duration::from_secs(1));
        egui::TopBottomPanel::top("header").show(ctx,|ui|{
            ui.add_space(8.0);
            ui.horizontal(|ui|{
                ui.colored_label(DARK,egui::RichText::new("TECHBASE").strong().size(19.0));
                ui.separator();
                ui.label(egui::RichText::new("SleufBase Rust").strong().size(18.0));
                ui.label(egui::RichText::new("Migratie-preview").color(Color32::GRAY));
                if self.dirty {ui.colored_label(DARK,"● Niet opgeslagen");}
                ui.separator();
                if ui.button("Nieuw project").clicked() {self.new_project();}
                if ui.button("Open Rust-project").clicked() {
                    if let Some(path)=rfd::FileDialog::new()
                        .add_filter("Rust-project JSON",&["json"]).pick_file()
                    {self.load_project_file(path,ctx);}
                }
                if ui.button("Opslaan").clicked() {self.save_project_file(false);}
                if ui.button("Opslaan als...").clicked() {self.save_project_file(true);}
            });
            ui.horizontal(|ui|{
                if ui.button("Open jobFeatures.json").clicked() {
                    if let Some(files)=rfd::FileDialog::new()
                        .add_filter("KickTheMap JSON",&["json"]).pick_files()
                    {self.load_files(files);}
                }
                if ui.button("Open GeoTIFF").clicked() {
                    if let Some(files)=rfd::FileDialog::new()
                        .add_filter("GeoTIFF",&["tif","tiff"]).pick_files()
                    {self.load_geotiffs(files,ctx);}
                }
                if ui.button("Open DXF").clicked() {
                    if let Some(files)=rfd::FileDialog::new()
                        .add_filter("AutoCAD DXF",&["dxf"]).pick_files()
                    {self.load_overlays(files);}
                }
                ui.separator();
                if ui.button("Excel disciplines").clicked() {self.save_excel();}
                if ui.button("DXF objecten").clicked() {self.save_dxf();}
                if ui.button("Kaart terugzetten").clicked() {self.zoom=1.0;self.pan=egui::Vec2::ZERO;}
            });
            if !self.message.is_empty() {
                ui.label(egui::RichText::new(&self.message).small().color(DARK));
            }
        });

        egui::SidePanel::left("sources")
            .default_width(300.0).min_width(230.0).show(ctx,|ui|{
            ui.heading("Ingeladen proefsleuven");
            ui.checkbox(&mut self.active_only,"Alleen actief in Python-SleufBase");
            if self.active_only {
                ui.small("Leest oude KickTheMap-state, maximaal 6 seconden oud.");
            }
            let active_ids=self.active_job_ids();
            ui.text_edit_singleline(&mut self.query);
            ui.small("Zoek op naam of job-ID");
            ui.separator();
            egui::ScrollArea::vertical().max_height(180.0).show(ui,|ui|{
                for (index,ds) in self.datasets.iter().enumerate() {
                    if self.active_only && !active_ids.contains(&ds.job_id) {continue;}
                    let search=self.query.trim().to_lowercase();
                    if !search.is_empty() && !ds.title.to_lowercase().contains(&search)
                        && !ds.job_id.to_string().contains(&search) {continue;}
                    let label=format!("{}  ({} objecten)",ds.title,ds.features.len());
                    if ui.selectable_label(self.selected==Some(index),label).clicked() {
                        self.selected=Some(index);
                    }
                }
            });
            ui.separator();
            ui.heading("GeoTIFF-bestanden");
            egui::ScrollArea::vertical().max_height(130.0).show(ui,|ui|{
                for info in &self.geotiffs {
                    ui.label(info.path.file_name().and_then(|s|s.to_str()).unwrap_or("TIFF"));
                    ui.small(format!("{}x{}, EPSG {}, job {}",
                        info.width,info.height,
                        info.epsg.map(|v|v.to_string()).unwrap_or_else(||"?".into()),
                        info.kickthemap_job_id.map(|v|v.to_string()).unwrap_or_else(||"?".into())));
                }
            });
            ui.separator();
            ui.heading("DXF-lagen");
            egui::ScrollArea::vertical().max_height(150.0).show(ui,|ui|{
                for overlay in &mut self.overlays {
                    let label=overlay.path.file_name().and_then(|s|s.to_str()).unwrap_or("DXF");
                    let changed=ui.checkbox(&mut overlay.visible,label).changed();
                    if changed {self.dirty=true;}
                    ui.small(format!("{} entiteiten",overlay.features.len()));
                }
            });
            ui.separator();
            if let Some(ds)=self.selected.and_then(|i|self.datasets.get(i)) {
                ui.strong(&ds.title);
                let summary=sleufbase_rust::summarize(ds,RULES);
                ui.label(format!("Job-ID {}, {} objecten, {} disciplines",
                    ds.job_id,ds.features.len(),summary.distinct_count()));
                for (rule,count) in RULES.iter().zip(summary.counts.iter()) {
                    if *count>0 {ui.label(format!("{}: {}",rule.label,count));}
                }
            }
        });

        egui::CentralPanel::default().show(ctx,|ui|{
            ui.heading("Kaart");
            ui.label("Zoom met scrollwiel, sleep om te verschuiven. Basis GeoTIFF en DXF zonder PDOK ondergrond.");
            let selected:Vec<Dataset>=self.selected.and_then(|i|self.datasets.get(i))
                .into_iter().cloned().collect();
            canvas(ui,&selected,&self.geotiffs,&self.raster_textures,
                &self.overlays,&mut self.zoom,&mut self.pan);
        });
    }
}
