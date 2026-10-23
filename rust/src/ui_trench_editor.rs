//! Native editor for virtual trenches and MarXact measurements.
//! All edits are staged: the current project changes only after validation.
use super::*;
use sleufbase_rust::{
    virtual_trench::{TrenchPoint,VirtualTrench},
    virtual_trench_editor as edit,
};
use egui::Vec2;
#[path="ui_trench_ahn.rs"]
mod ahn_fill;

#[derive(Clone)]
struct ObjectForm {
    role:String,
    name:String,
    source:String,
    attributes:[String;3],
    chainage:f64,
    offset:f64,
    elevation:f64,
    has_elevation:bool,
}

#[cfg(test)]
mod metadata_tests {
    use super::*;
    #[test]
    fn clearing_source_references_and_notes_cannot_be_undone_by_saving_an_open_draft(){
        let mut t=VirtualTrench::new("Test".into(),Vertex{x:155000.,y:463000.,z:Some(2.)},Vertex{x:155010.,y:463000.,z:Some(2.)},0.6).unwrap();
        t.metadata=serde_json::json!({"template_reference_point":{"x":155001.,"y":463001.},"tiff_comment_text":"Note","keep":"Ø"});
        let mut app=SleufBaseApp{virtual_trenches:vec![t],..Default::default()};app.open_virtual_editor(0);
        app.trench_editor.as_mut().unwrap().add_or_replace().unwrap();
        app.clear_raster_references();app.clear_map_comments();
        let next=app.trench_editor.take().unwrap().build().unwrap();
        assert_eq!(next.objects.len(),1);assert_eq!(next.metadata["keep"],"Ø");assert!(next.metadata.get("template_reference_point").is_none());assert!(next.metadata.get("tiff_comment_text").is_none());
    }
}
impl Default for ObjectForm {
    fn default()->Self {
        Self {
            role:"kabel".into(),name:"LS".into(),source:"LS".into(),
            attributes:Default::default(),
            chainage:0.,offset:0.,elevation:0.,has_elevation:false,
        }
    }
}
impl ObjectForm {
    fn from_object(trench:&VirtualTrench,object:&TrenchPoint)->Self {
        let (chainage,offset)=edit::coordinates(trench,object.position);
        Self {
            role:object.role.clone(),name:object.object_name.clone(),
            source:object.source_name.clone(),
            attributes:object.attributes.clone(),chainage,offset,
            elevation:object.position.z.unwrap_or(0.),
            has_elevation:object.position.z.is_some(),
        }
    }
    fn to_object(&self,trench:&VirtualTrench)->Result<TrenchPoint,String> {
        Ok(TrenchPoint {metadata:Default::default(),
            role:self.role.clone(),object_name:self.name.clone(),
            source_name:self.source.clone(),
            attributes:self.attributes.clone(),
            position:edit::position(trench,self.chainage,self.offset,
                self.has_elevation.then_some(self.elevation))?,
        })
    }
}

pub(super) struct Editor {
    ahn_job:Option<ahn_fill::Job>,
    pub index:usize,
    original:VirtualTrench,
    draft:VirtualTrench,
    label:String,
    start:Vertex,
    end:Vertex,
    width:f64,
    selected:Option<usize>,
    object:ObjectForm,
    confirm_delete:bool,
    error:String,
    paste:String,
    notice:String,
    shift_x:f64,
    shift_y:f64,
    dekband_start:f64,
    dekband_end:f64,
    dekband_name:String,
    dekband_rgb:[u8;3],
    dekband_start_depth:f64,
    dekband_end_depth:f64,
}
#[derive(Clone, Copy)]
enum Action {Save,Copy,Remove}

impl Editor {
    fn new(index:usize,source:&VirtualTrench)->Self {
        Self{
            ahn_job:None,
            index,original:source.clone(),draft:source.clone(),
            label:source.label.clone(),start:source.start,
            end:source.end,width:source.width_meters,
            selected:None,object:ObjectForm::default(),
            confirm_delete:false,error:String::new(),paste:String::new(),notice:String::new(),shift_x:0.,shift_y:0.,
            dekband_start:0.,dekband_end:source.length(),
            dekband_name:"Dekband".into(),dekband_rgb:[17,24,39],
            dekband_start_depth:0.,dekband_end_depth:0.,
        }
    }
    fn build(&self)->Result<VirtualTrench,String> {
        let mut result=self.draft.clone();
        edit::edit_axis(&mut result,self.label.clone(),self.start,self.end,self.width)?;
        edit::check(&result)?;
        Ok(result)
    }
    fn add_or_replace(&mut self)->Result<(),String> {
        let mut next=self.build()?;
        let mut object=self.object.to_object(&next)?;
        if let Some(old)=self.selected.and_then(|i|next.objects.get(i)){
            object.metadata=old.metadata.clone();
            if object.position.z!=old.position.z {
                let meta=object.metadata.as_object_mut();
                if let Some(meta)=meta {
                    meta.remove("sleufbase_depth_meters");
                    meta.insert("surface_mode".into(),serde_json::json!("manual"));
                    meta.insert("surface_status".into(),serde_json::json!("manual"));
                }
            }
        }
        edit::replace_object(&mut next,self.selected,object)?;
        // The axis still belongs to the form, but the edited objects are
        // stored as RD world coordinates and will survive later translations.
        self.draft=next;
        self.selected=None;
        self.object=ObjectForm::default();
        Ok(())
    }
    fn draw_preview(&self,ui:&mut egui::Ui) {
        // Never clone/validate thousands of measured objects per animation
        // frame. The project stays untouched until the Save action.
        let t=&self.draft;
        let start=self.start;
        let end=self.end;
        let width=self.width;
        let len=(end.x-start.x).hypot(end.y-start.y);
        let outline=if edit::is_measured(t) {
            t.measured_boundary()
        } else {
            let denom=len.max(1e-9);
            let nx=-(end.y-start.y)/denom*width.max(0.1)*0.5;
            let ny=(end.x-start.x)/denom*width.max(0.1)*0.5;
            let mut points=vec![
                (start.x+nx,start.y+ny),(start.x-nx,start.y-ny),
                (end.x-nx,end.y-ny),(end.x+nx,end.y+ny)
            ];
            points.push(points[0]);
            points
        };
        let (rect,_response)=ui.allocate_exact_size(
            Vec2::new(ui.available_width().max(240.),190.),egui::Sense::hover()
        );
        let painter=ui.painter_at(rect);
        painter.rect_filled(rect,6.,Color32::from_rgb(250,250,250));
        let stride=(t.objects.len()/2048).max(1);
        let sampled=t.objects.iter().step_by(stride).collect::<Vec<_>>();
        let mut xs=vec![start.x,end.x];
        let mut ys=vec![start.y,end.y];
        for &(x,y) in &outline {xs.push(x);ys.push(y);}
        for obj in &sampled {xs.push(obj.position.x);ys.push(obj.position.y);}
        let min_x=xs.iter().copied().fold(f64::INFINITY,f64::min);
        let max_x=xs.iter().copied().fold(f64::NEG_INFINITY,f64::max);
        let min_y=ys.iter().copied().fold(f64::INFINITY,f64::min);
        let max_y=ys.iter().copied().fold(f64::NEG_INFINITY,f64::max);
        let w=(max_x-min_x).max(1.0);
        let h=(max_y-min_y).max(1.0);
        let scale=((rect.width()-50.) as f64/w).min((rect.height()-42.) as f64/h);
        let center_x=(min_x+max_x)*0.5;
        let center_y=(min_y+max_y)*0.5;
        let center=rect.center();
        let point=|x:f64,y:f64|Pos2::new(
            center.x+((x-center_x)*scale) as f32,
            center.y-((y-center_y)*scale) as f32
        );
        for line in outline.windows(2) {
            painter.line_segment([point(line[0].0,line[0].1),point(line[1].0,line[1].1)],
                Stroke::new(2_f32,Color32::from_rgb(75,85,99)));
        }
        painter.line_segment([point(start.x,start.y),point(end.x,end.y)],
            Stroke::new(2.2_f32,ORANGE));
        if len>1e-9 {
            let stride=(t.dekband_rows.len()/1024).max(1);
            for band in t.dekband_rows.iter().step_by(stride) {
                let start_m=band.start_chainage.clamp(0.,len);
                let end_m=band.end_chainage.clamp(0.,len);
                if end_m<=start_m {continue;}
                let at=|m:f64|{
                    let f=m/len;
                    point(start.x+(end.x-start.x)*f,start.y+(end.y-start.y)*f)
                };
                let c=band.display_rgb.unwrap_or([17,24,39]);
                painter.line_segment([at(start_m),at(end_m)],
                    Stroke::new(3_f32,Color32::from_rgb(c[0],c[1],c[2])));
            }
        }
        painter.circle_filled(point(start.x,start.y),4.,Color32::GREEN);
        painter.circle_filled(point(end.x,end.y),4.,Color32::RED);
        for obj in sampled {
            painter.circle_filled(point(obj.position.x,obj.position.y),3.,INK);
        }
        if let Some(i)=self.selected {
            if let Some(obj)=t.objects.get(i) {
                painter.circle_filled(point(obj.position.x,obj.position.y),5.,ORANGE);
            }
        }
        painter.text(rect.left_top()+Vec2::new(8.,6.),egui::Align2::LEFT_TOP,
            format!("Bovenaanzicht | lengte {:.2} m | breedte {:.2} m | {} objecten",
                len,width,t.objects.len()),
            egui::FontId::proportional(11.),INK);
    }
}
impl SleufBaseApp {
    pub(super) fn clear_virtual_editor_metadata(&mut self,key:&str,index:Option<usize>){
        if let Some(editor)=self.trench_editor.as_mut().filter(|e|index.is_none_or(|i|i==e.index)){
            for t in [&mut editor.original,&mut editor.draft]{if let Some(m)=t.metadata.as_object_mut(){m.remove(key);}}
        }
    }
    pub(super) fn open_virtual_editor(&mut self,index:usize) {
        self.cancel_virtual_map();
        if self.export_intent.is_some() || self.template_job.is_some()
            || self.file_intent.is_some() {
            self.message="Rond de lopende laad- of exporttaak eerst af".into();
            return;
        }
        if let Some(t)=self.virtual_trenches.get(index) {
            self.trench_editor=Some(Editor::new(index,t));
        }
    }
    pub(super) fn show_virtual_editor(&mut self,ctx:&egui::Context) {
        let Some(mut editor)=self.trench_editor.take() else{return};
        if editor.index>=self.virtual_trenches.len() {return;}
        editor.poll_ahn();
        let mut open=true;
        let mut action=None;
        egui::Window::new("Virtuele proefsleuf en meetobjecten bewerken")
            .open(&mut open).default_width(730.)
            .default_height(650.)
            .resizable(true).show(ctx,|ui|{
                egui::ScrollArea::vertical().max_height(660.).show(ui,|ui|{
                    editor.ahn_controls(ui,ctx);
                    ui.add_enabled_ui(editor.ahn_job.is_none(),|ui|{
                    let measured=edit::is_measured(&editor.original);
                    ui.heading("Proefsleuf");
                    ui.horizontal(|ui|{
                        ui.label("Naam");
                        ui.text_edit_singleline(&mut editor.label);
                        ui.label(format!("Job {}",editor.original.job_id));
                    });
                    if measured {
                        ui.weak("MarXact: originele 3D-meetgrenzen en uitlijning blijven beschermd. Gebruik 'Handmatige kopie' om de as te wijzigen.");
                    }
                    ui.add_enabled_ui(!measured,|ui|{
                        ui.horizontal(|ui|{
                            ui.label("Begin RD X");
                            ui.add(egui::DragValue::new(&mut editor.start.x).speed(0.1));
                            ui.label("Y");
                            ui.add(egui::DragValue::new(&mut editor.start.y).speed(0.1));
                        });
                        ui.horizontal(|ui|{
                            ui.label("Einde RD X");
                            ui.add(egui::DragValue::new(&mut editor.end.x).speed(0.1));
                            ui.label("Y");
                            ui.add(egui::DragValue::new(&mut editor.end.y).speed(0.1));
                        });
                        ui.horizontal(|ui|{
                            ui.label("Breedte (m)");
                            ui.add(egui::DragValue::new(&mut editor.width).range(0.1..=100.).speed(0.05));
                        });
                        ui.horizontal(|ui|{
                            let old_start=editor.start.z;
                            let old_end=editor.end.z;
                            let mut has_begin=editor.start.z.is_some();
                            let mut begin_z=editor.start.z.unwrap_or(0.);
                            ui.checkbox(&mut has_begin,"Beginhoogte Z");
                            ui.add_enabled(has_begin,egui::DragValue::new(&mut begin_z).speed(0.05));
                            editor.start.z=has_begin.then_some(begin_z);
                            let mut has_end=editor.end.z.is_some();
                            let mut end_z=editor.end.z.unwrap_or(0.);
                            ui.checkbox(&mut has_end,"Eindhoogte Z");
                            ui.add_enabled(has_end,egui::DragValue::new(&mut end_z).speed(0.05));
                            editor.end.z=has_end.then_some(end_z);
                            if old_start!=editor.start.z {editor.mark_endpoint_height(true);}
                            if old_end!=editor.end.z {editor.mark_endpoint_height(false);}
                        });
                        if ui.button("Begin- en eindpunt omkeren").clicked() {
                            match editor.build() {
                                Ok(mut next)=>match edit::reverse_axis(&mut next) {
                                    Ok(())=>{
                                        editor.start=next.start;
                                        editor.end=next.end;
                                        editor.draft=next;
                                        editor.error.clear();
                                    },
                                    Err(e)=>editor.error=e,
                                },
                                Err(e)=>editor.error=e,
                            }
                        }
                        ui.horizontal(|ui|{
                            ui.label("Verschuif gehele sleuf RD ΔX");
                            ui.add(egui::DragValue::new(&mut editor.shift_x).speed(0.1));
                            ui.label("ΔY");
                            ui.add(egui::DragValue::new(&mut editor.shift_y).speed(0.1));
                            if ui.small_button("Pas verplaatsing toe").clicked() {
                                match editor.build() {
                                    Ok(mut next)=>match edit::translate(&mut next,editor.shift_x,editor.shift_y) {
                                        Ok(())=>{
                                            editor.start=next.start;
                                            editor.end=next.end;
                                            editor.draft=next;
                                            editor.shift_x=0.;
                                            editor.shift_y=0.;
                                            editor.error.clear();
                                        },
                                        Err(e)=>editor.error=e,
                                    },
                                    Err(e)=>editor.error=e,
                                }
                            }
                        });
                    });
                    editor.draw_preview(ui);
                    ui.separator();
                    ui.collapsing(format!("Dekbandsegmenten ({})",editor.draft.dekband_rows.len()),|ui|{
                        ui.small("De dekband volgt de meetas tussen twee kettingmaten en verschijnt als gekleurde langsstreep in het gegenereerde virtuele raster.");
                        ui.horizontal(|ui|{
                            ui.label("Van (m)");
                            ui.add(egui::DragValue::new(&mut editor.dekband_start).speed(0.05));
                            ui.label("Tot (m)");
                            ui.add(egui::DragValue::new(&mut editor.dekband_end).speed(0.05));
                            ui.label("Beschrijving");
                            ui.text_edit_singleline(&mut editor.dekband_name);
                        });
                        ui.horizontal(|ui|{
                            ui.label("Kleur");
                            ui.color_edit_button_srgb(&mut editor.dekband_rgb);
                            ui.label("Diepte begin/eind (m)");
                            ui.add(egui::DragValue::new(&mut editor.dekband_start_depth).range(0.0..=1000.).speed(0.05));
                            ui.add(egui::DragValue::new(&mut editor.dekband_end_depth).range(0.0..=1000.).speed(0.05));
                            if ui.add_enabled(editor.draft.dekband_rows.len()<10_000,
                                egui::Button::new("Voeg dekband toe")).clicked() {
                                let segment=sleufbase_rust::virtual_trench::Dekband{start_depth:editor.dekband_start_depth,end_depth:editor.dekband_end_depth,
                                    start_chainage:editor.dekband_start,
                                    end_chainage:editor.dekband_end,
                                    source_name:editor.dekband_name.clone(),
                                    display_rgb:Some(editor.dekband_rgb),
                                };
                                let mut next=editor.draft.clone();
                                next.dekband_rows.push(segment);
                                match edit::check(&next) {
                                    Ok(())=>{
                                        editor.draft=next;
                                        editor.error.clear();
                                    },
                                    Err(err)=>editor.error=err,
                                }
                            }
                        });
                        let mut remove=None;
                        egui::ScrollArea::vertical().max_height(170.).show(ui,|ui|{
                            for (i,band) in editor.draft.dekband_rows.iter_mut().enumerate(){
                                ui.push_id(i,|ui|{
                                    ui.horizontal(|ui|{
                                        ui.label(format!("{}:",i+1));
                                        ui.add(egui::DragValue::new(&mut band.start_chainage).speed(0.05));
                                        ui.label("–");
                                        ui.add(egui::DragValue::new(&mut band.end_chainage).speed(0.05));
                                        ui.label("Diepte");
                                        ui.add(egui::DragValue::new(&mut band.start_depth).range(0.0..=1000.).speed(0.05));
                                        ui.add(egui::DragValue::new(&mut band.end_depth).range(0.0..=1000.).speed(0.05));
                                        ui.text_edit_singleline(&mut band.source_name);
                                        let rgb=band.display_rgb.get_or_insert([17,24,39]);
                                        ui.color_edit_button_srgb(rgb);
                                        if ui.small_button("Verwijder").clicked(){remove=Some(i);}
                                    });
                                });
                            }
                        });
                        if let Some(index)=remove{editor.draft.dekband_rows.remove(index);}
                    });
                    ui.separator();
                    ui.heading("Meetobjecten");
                    ui.small("Kettingmaat: meters vanaf het beginpunt. Dwarsafstand: positief links van de meetas.");
                    ui.horizontal(|ui|{
                        ui.label("Object");
                        ui.text_edit_singleline(&mut editor.object.name);
                        ui.label("Bron");
                        ui.text_edit_singleline(&mut editor.object.source);
                    });
                    ui.horizontal(|ui|{
                        ui.label("Rol");
                        ui.text_edit_singleline(&mut editor.object.role);
                    });
                    ui.horizontal(|ui|{
                        ui.label("Kettingmaat");
                        ui.add(egui::DragValue::new(&mut editor.object.chainage).speed(0.05));
                        ui.label("Dwarsafstand");
                        ui.add(egui::DragValue::new(&mut editor.object.offset).speed(0.02));
                    });
                    ui.horizontal(|ui|{
                        ui.checkbox(&mut editor.object.has_elevation,"Z-hoogte");
                        ui.add_enabled(editor.object.has_elevation,
                            egui::DragValue::new(&mut editor.object.elevation).speed(0.05));
                    });
                    ui.collapsing("Objectattributen (3 velden)",|ui|{
                        for attribute in &mut editor.object.attributes {
                            ui.text_edit_singleline(attribute);
                        }
                    });
                    ui.horizontal(|ui|{
                        if ui.button(if editor.selected.is_some() {"Wijzig meetobject"}else{"Meetobject toevoegen"}).clicked() {
                            match editor.add_or_replace() {
                                Ok(())=>editor.error.clear(),
                                Err(e)=>editor.error=e,
                            }
                        }
                        if ui.button("Leeg invoerformulier").clicked() {
                            editor.selected=None;
                            editor.object=ObjectForm::default();
                        }
                    });
                    let mut remove=None;
                    egui::ScrollArea::vertical().max_height(190.)
                        .show_rows(ui,24.,editor.draft.objects.len(),|ui,rows|{
                            for index in rows {
                                let obj=&editor.draft.objects[index];
                                let (c,o)=edit::coordinates(&editor.draft,obj.position);
                                ui.horizontal(|ui|{
                                    ui.label(format!("{:>3}  {} — {:.2} m / {:+.2} m",
                                        index+1,obj.object_name,c,o));
                                    if ui.small_button("Bewerk").clicked() {
                                        editor.selected=Some(index);
                                        editor.object=ObjectForm::from_object(&editor.draft,obj);
                                    }
                                    if ui.small_button("Verwijder").clicked(){remove=Some(index);}
                                });
                            }
                        });
                    if let Some(index)=remove {
                        if let Err(e)=edit::remove_object(&mut editor.draft,index){editor.error=e;}
                        editor.selected=None;
                        editor.object=ObjectForm::default();
                    }
                    ui.collapsing("Excel: meerdere meetobjecten importeren / kopiëren",|ui|{
                        ui.small("Plak tab-gescheiden cellen uit Excel: Rol, Object, Bron, Kettingmaat, Dwarsafstand, optionele Z en 3 attributen. Komma als decimaalteken is toegestaan.");
                        ui.add(egui::TextEdit::multiline(&mut editor.paste)
                            .desired_rows(5).hint_text("LS\\tKabel\\tKickTheMap\\t2,5\\t-0,3\\t-1,2"));
                        if ui.button("Importeer geplakte Excel-regels").clicked() {
                            match editor.build(){
                                Ok(mut next)=>match edit::import_tabular(&mut next,&editor.paste) {
                                    Ok(count)=>{
                                        editor.draft=next;
                                        editor.error.clear();
                                        editor.notice=format!("{count} meetobjecten toegevoegd; klik op Opslaan in project om definitief te bewaren.");
                                        editor.paste.clear();
                                    }
                                    Err(e)=>editor.error=e,
                                },
                                Err(e)=>editor.error=e,
                            }
                        }
                        if ui.button("Kopieer alle meetobjecten naar Excel").clicked() {
                            match editor.build().and_then(|t|edit::export_tabular(&t)){
                                Ok(table)=>ui.ctx().copy_text(table),
                                Err(e)=>editor.error=e,
                            }
                        }
                    });
                    ui.separator();
                    if !editor.error.is_empty() {
                        ui.colored_label(DARK,&editor.error);
                    }
                    if !editor.notice.is_empty() {ui.small(&editor.notice);}
                    ui.horizontal(|ui|{
                        if ui.button("Opslaan in project").clicked() {action=Some(Action::Save);}
                        if ui.button("Handmatige kopie").clicked() {action=Some(Action::Copy);}
                        if ui.button("Proefsleuf verwijderen…").clicked(){editor.confirm_delete=true;}
                    });
                    if editor.confirm_delete {
                        ui.colored_label(DARK,"Alleen deze proefsleuf verwijderen? Alle meetobjecten ervan worden ook verwijderd.");
                        ui.horizontal(|ui|{
                            if ui.button("Ja, verwijderen").clicked(){action=Some(Action::Remove);}
                            if ui.button("Annuleren").clicked(){editor.confirm_delete=false;}
                        });
                    }
                    });
                });
            });
        if let Some(cmd)=action {
            match cmd {
                Action::Save=>{
                    match editor.build() {
                        Ok(next)=>{
                            self.virtual_trenches[editor.index]=next;
                            self.trench_markers.clear();
                            self.dirty=true;
                            self.message="Virtuele proefsleuf en meetobjecten opgeslagen".into();
                            return;
                        },
                        Err(e)=>editor.error=e,
                    }
                },
                Action::Copy=>{
                    match editor.build().and_then(|t|edit::manual_copy(&t,"(handmatig)")){
                        Ok(copy)=>{
                            self.virtual_trenches.push(copy);
                            self.trench_markers.clear();
                            self.dirty=true;
                            self.message="Handmatige kopie gemaakt; de originele MarXact-meting is behouden".into();
                            self.trench_editor=Some(Editor::new(self.virtual_trenches.len()-1,
                                self.virtual_trenches.last().unwrap()));
                            return;
                        },
                        Err(e)=>editor.error=e,
                    }
                },
                Action::Remove=>{
                    self.virtual_trenches.remove(editor.index);
                    self.trench_markers.clear();
                    self.refresh_virtual_references();
                    self.dirty=true;
                    self.message="Virtuele proefsleuf verwijderd".into();
                    return;
                },
            }
        }
        if open {self.trench_editor=Some(editor);}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn renaming_keeps_start_depth_but_editing_z_protects_manual_measurement(){
        let mut trench=VirtualTrench::new("PS1".into(),Vertex{x:155000.,y:463000.,z:Some(5.)},Vertex{x:155010.,y:463000.,z:Some(5.)},0.6).unwrap();
        trench.objects.push(TrenchPoint{role:"object".into(),object_name:"LS".into(),source_name:"LS".into(),position:Vertex{x:155002.,y:463000.,z:Some(4.)},attributes:Default::default(),metadata:serde_json::json!({"surface_mode":"start","surface_z":5.,"sleufbase_depth_meters":1.,"point_id":"source-1"})});
        let mut editor=Editor::new(0,&trench);editor.selected=Some(0);editor.object=ObjectForm::from_object(&trench,&trench.objects[0]);editor.object.name="Hernoemd".into();
        editor.add_or_replace().unwrap();assert_eq!(editor.draft.objects[0].metadata["sleufbase_depth_meters"],1.);assert_eq!(editor.draft.objects[0].metadata["surface_mode"],"start");
        editor.selected=Some(0);editor.object=ObjectForm::from_object(&editor.draft,&editor.draft.objects[0]);editor.object.elevation=3.25;editor.add_or_replace().unwrap();
        assert_eq!(editor.draft.objects[0].metadata["surface_mode"],"manual");assert_eq!(editor.draft.objects[0].metadata["point_id"],"source-1");
        editor.draft.start_metadata=serde_json::json!({"surface_mode":"auto"});
        sleufbase_rust::virtual_map::apply_surface_height(&mut editor.draft,sleufbase_rust::virtual_map::MapPoint::Start,Some(8.)).unwrap();
        assert_eq!(editor.draft.objects[0].position.z,Some(3.25));
    }
    #[test]
    fn dialog_stages_edits_without_changing_live_project_until_commit() {
        let mut app=SleufBaseApp::default();
        app.virtual_trenches.push(VirtualTrench::new("Before".into(),
            Vertex{x:100.,y:200.,z:None},Vertex{x:110.,y:200.,z:None},0.6).unwrap());
        app.open_virtual_editor(0);
        let mut editor=app.trench_editor.take().unwrap();
        editor.label="After".into();
        editor.object.chainage=3.;
        editor.object.offset=0.2;
        editor.add_or_replace().unwrap();
        assert_eq!(app.virtual_trenches[0].label,"Before");
        assert!(app.virtual_trenches[0].objects.is_empty());
        let next=editor.build().unwrap();
        app.virtual_trenches[0]=next;
        assert_eq!(app.virtual_trenches[0].label,"After");
        assert_eq!(app.virtual_trenches[0].objects.len(),1);
        assert!((app.virtual_trenches[0].objects[0].position.y-200.2).abs()<1e-9);
    }
}
