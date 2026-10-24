//! Original TIFF-address overview and clipboard actions; never owns pixels.
use super::*;
use sleufbase_rust::raster_addresses::{self, Record, Worker};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[derive(Default)]
pub(super) struct Dialog {
    pub open: bool,
    worker: Option<Worker>,
    ticket: Option<u64>,
    progress: Option<Arc<AtomicUsize>>,
    total: usize,
    records: Vec<Record>,
    selected: Option<usize>,
    error: Option<String>,
}
impl Dialog {
    pub fn reset(&mut self) {
        self.cancel();
        let worker = self.worker.take();
        *self = Self {
            worker,
            ..Default::default()
        };
    }
    fn cancel(&mut self) {
        if let Some(worker) = &self.worker {
            worker.cancel();
        }
        self.ticket = None;
        self.progress = None;
    }
    fn accept(&mut self, ready: raster_addresses::Ready) {
        if !self.open || self.ticket != Some(ready.ticket) {
            return;
        }
        self.ticket = None;
        self.progress = None;
        match ready.result {
            Ok(records) => {
                self.records = records;
                self.selected = (!self.records.is_empty()).then_some(0);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }
    fn start(&mut self, sources: Vec<raster_addresses::Source>) -> Result<(), String> {
        self.cancel();
        self.total = sources.len();
        self.records.clear();
        self.selected = None;
        self.error = None;
        if self.worker.is_none() {
            self.worker = Some(Worker::new()?);
        }
        let ticket = self.worker.as_ref().unwrap().start(sources)?;
        self.ticket = Some(ticket.id);
        self.progress = Some(ticket.progress);
        Ok(())
    }
}
impl SleufBaseApp {
    pub(super) fn open_tiff_addresses(&mut self) {
        if self.geotiffs.is_empty() {
            self.message = "Laad eerst minimaal een GeoTIFF-bestand.".into();
            return;
        }
        self.tiff_addresses.open = true;
        let sources = self
            .geotiffs
            .iter()
            .map(raster_addresses::Source::from_raster)
            .collect();
        if let Err(error) = self.tiff_addresses.start(sources) {
            self.tiff_addresses.error = Some(error);
        }
    }
    pub(super) fn poll_tiff_addresses(&mut self, ctx: &egui::Context) {
        if let Some(ready) = self.tiff_addresses.worker.as_ref().and_then(Worker::poll) {
            let accepted =
                self.tiff_addresses.open && self.tiff_addresses.ticket == Some(ready.ticket);
            self.tiff_addresses.accept(ready);
            if accepted {
                self.message = self.tiff_addresses.error.as_ref().map_or_else(
                    || {
                        format!(
                            "Adressen opgehaald voor {} GeoTIFF-bestand(en).",
                            self.tiff_addresses.records.len()
                        )
                    },
                    |e| format!("TIFF-adressen ophalen mislukt: {e}"),
                );
            }
        }
        if self.tiff_addresses.ticket.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }
    fn copy_tiff_address(&mut self, ctx: &egui::Context, index: usize, include_name: bool) {
        if let Some(row) = self.tiff_addresses.records.get(index) {
            ctx.copy_text(row.copied(include_name).to_owned());
            self.message = format!("Adres gekopieerd: {}", row.name);
        }
    }
    pub(super) fn show_tiff_addresses(&mut self, ctx: &egui::Context) {
        if !self.tiff_addresses.open {
            return;
        }
        let mut open = true;
        let mut cancel = false;
        let mut refresh = false;
        let mut copy = None;
        egui::Window::new("TIFF-adressen").id(egui::Id::new("tiff-addresses"))
            .open(&mut open).default_size(egui::vec2(850.,400.)).show(ctx,|ui|{
            ui.label("GeoTIFF-adressen via PDOK reverse geocoder");
            ui.small("Adressen horen bij de GeoTIFFs die bij het openen geladen waren. Selecteer een regel om te kopiëren.");
            if let Some(progress)=&self.tiff_addresses.progress{
                let done=progress.load(Ordering::Relaxed).min(self.tiff_addresses.total);
                ui.add(egui::ProgressBar::new(done as f32/self.tiff_addresses.total.max(1) as f32)
                    .text(format!("Adressen ophalen: {done} / {}",self.tiff_addresses.total)));
                if ui.button("Ophalen annuleren").clicked(){cancel=true;}
            }
            if let Some(error)=&self.tiff_addresses.error{ui.colored_label(DARK,error);}
            egui::ScrollArea::both().max_height(300.).show(ui,|ui|{
                egui::Grid::new("tiff-address-rows").striped(true).num_columns(3).show(ui,|ui|{
                    ui.strong("Afbeelding");ui.strong("Adres");ui.strong("Afstand");ui.end_row();
                    for (i,row) in self.tiff_addresses.records.iter().enumerate(){
                        let response=ui.selectable_label(self.tiff_addresses.selected==Some(i),cad_preview(&row.name));
                        if response.clicked(){self.tiff_addresses.selected=Some(i);}
                        if response.double_clicked(){copy=Some((i,false));}
                        ui.label(cad_preview(&row.address));ui.label(&row.distance);ui.end_row();
                    }
                });
            });
            ui.horizontal(|ui|{
                let selected=self.tiff_addresses.selected;
                if ui.add_enabled(selected.is_some(),egui::Button::new("Kopieer adres")).clicked(){copy=selected.map(|i|(i,false));}
                if ui.add_enabled(selected.is_some(),egui::Button::new("Kopieer met bestandsnaam")).clicked(){copy=selected.map(|i|(i,true));}
                if ui.button("Vernieuwen").clicked(){refresh=true;}
            });
        });
        if let Some((index, include_name)) = copy {
            self.copy_tiff_address(ctx, index, include_name);
        }
        if cancel {
            self.tiff_addresses.cancel();
            self.message = "TIFF-adressen ophalen geannuleerd".into();
        }
        if refresh {
            self.open_tiff_addresses();
        }
        if !open {
            self.tiff_addresses.open = false;
            self.tiff_addresses.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sleufbase_rust::location_search::ReverseLocation;
    fn row() -> Record {
        Record::from_result(
            "Proefsleuf Ø.tif".into(),
            Ok(vec![ReverseLocation {
                label: "Straat Ø 12, Testplaats".into(),
                distance_m: 1.25,
                location_type: "adres".into(),
            }]),
        )
    }
    #[test]
    fn actual_egui_clipboard_commands_use_original_address_and_filename_text() {
        let ctx = egui::Context::default();
        let mut app = SleufBaseApp {
            tiff_addresses: Dialog {
                open: true,
                records: vec![row()],
                selected: Some(0),
                ..Default::default()
            },
            ..Default::default()
        };
        for include_name in [false, true] {
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                app.show_tiff_addresses(ctx);
                app.copy_tiff_address(ctx, 0, include_name);
            });
            let copies = output
                .platform_output
                .commands
                .iter()
                .filter_map(|c| match c {
                    egui::OutputCommand::CopyText(text) => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(copies, vec![row().copied(include_name)]);
            assert_eq!(app.message, "Adres gekopieerd: Proefsleuf Ø.tif");
            assert!(!app.dirty);
        }
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            app.copy_tiff_address(ctx, 99, false)
        });
        assert!(output.platform_output.commands.is_empty());
    }
    #[test]
    fn cancelled_closed_and_project_switched_dialogs_ignore_late_rows_and_keep_the_worker() {
        let mut dialog = Dialog {
            open: true,
            ticket: Some(2),
            ..Default::default()
        };
        dialog.accept(raster_addresses::Ready {
            ticket: 1,
            result: Ok(vec![row()]),
        });
        assert!(dialog.records.is_empty());
        dialog.accept(raster_addresses::Ready {
            ticket: 2,
            result: Ok(vec![row()]),
        });
        assert_eq!(dialog.selected, Some(0));
        dialog.ticket = Some(3);
        dialog.open = false;
        dialog.accept(raster_addresses::Ready {
            ticket: 3,
            result: Err("old error".into()),
        });
        assert!(dialog.error.is_none());
        let worker = Worker::with_lookup(|_, _, _| Ok(Vec::new())).unwrap();
        let mut app = SleufBaseApp {
            tiff_addresses: Dialog {
                open: true,
                worker: Some(worker),
                records: vec![row()],
                ticket: Some(3),
                ..Default::default()
            },
            ..Default::default()
        };
        app.apply_project(
            NativeProject::default(),
            None,
            false,
            Vec::new(),
            &egui::Context::default(),
        );
        assert!(
            !app.tiff_addresses.open
                && app.tiff_addresses.records.is_empty()
                && app.tiff_addresses.ticket.is_none()
        );
        assert!(app.tiff_addresses.worker.is_some());
        assert!(!app.dirty);
        app.open_tiff_addresses();
        assert!(!app.tiff_addresses.open);
        assert_eq!(app.message, "Laad eerst minimaal een GeoTIFF-bestand.");
    }
    #[test]
    fn dialog_worker_queries_a_real_geotiff_center_and_preserves_loaded_sources() {
        use tiff::{
            encoder::{colortype::RGB8, TiffEncoder},
            tags::Tag,
        };
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("Voorbeeld Ø.tif");
        {
            let mut encoder = TiffEncoder::new(std::fs::File::create(&path).unwrap()).unwrap();
            let mut image = encoder.new_image::<RGB8>(12, 8).unwrap();
            image
                .encoder()
                .write_tag(Tag::ModelPixelScaleTag, &[0.5f64, 0.25, 0.][..])
                .unwrap();
            image
                .encoder()
                .write_tag(
                    Tag::ModelTiepointTag,
                    &[0f64, 0., 0., 168000., 401000., 0.][..],
                )
                .unwrap();
            image
                .encoder()
                .write_tag(
                    Tag::GeoKeyDirectoryTag,
                    &[1u16, 1, 0, 1, 3072, 0, 1, 28992][..],
                )
                .unwrap();
            image.write_data(&[255; 12 * 8 * 3]).unwrap();
        }
        let raster = sleufbase_rust::geotiff::read_geotiff_metadata(&path).unwrap();
        let worker = Worker::with_lookup(|x, y, _| {
            assert_eq!([x, y], [168003., 400999.]);
            Ok(vec![ReverseLocation {
                label: "Gevonden adres".into(),
                distance_m: 0.25,
                location_type: "adres".into(),
            }])
        })
        .unwrap();
        let mut app = SleufBaseApp {
            geotiffs: vec![raster],
            tiff_addresses: Dialog {
                worker: Some(worker),
                ..Default::default()
            },
            ..Default::default()
        };
        let sources = app.geotiffs.as_ptr();
        app.open_tiff_addresses();
        let ctx = egui::Context::default();
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.tiff_addresses.ticket.is_some() {
            app.poll_tiff_addresses(&ctx);
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(app.tiff_addresses.records[0].name, "Voorbeeld Ø.tif");
        assert_eq!(app.tiff_addresses.records[0].address, "Gevonden adres");
        assert_eq!(
            app.message,
            "Adressen opgehaald voor 1 GeoTIFF-bestand(en)."
        );
        assert_eq!(app.geotiffs.as_ptr(), sources);
        assert!(!app.dirty && app.raster_textures.is_empty());
    }
}
