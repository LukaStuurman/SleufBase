//! Native editor for virtual trenches and MarXact measurements.
//! All edits are staged: the current project changes only after validation.
use super::*;
use sleufbase_rust::{
    virtual_trench::{TrenchPoint,VirtualTrench},
    virtual_trench_editor as edit,
};
use egui::Vec2;

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
        Ok(TrenchPoint {
            role:self.role.clone(),object_name:self.name.clone(),
            source_name:self.source.clone(),
            attributes:self.attributes.clone(),
            position:edit::position(trench,self.chainage,self.offset,
                self.has_elevation.then_some(self.elevation))?,
        })
    }
}

#[derive(Clone)]
pub(super) struct Editor {
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
}
#[derive(Clone, Copy)]
enum Action {Save,Copy,Remove}

impl Editor {
    fn new(index:usize,source:&VirtualTrench)->Self {
        Self{
            index,original:source.clone(),draft:source.clone(),
            label:source.label.clone(),start:source.start,
            end:source.end,width:source.width_meters,
            selected:None,object:ObjectForm::default(),
            confirm_delete:false,error:String::new(),paste:String::new(),notice:String::new(),shift_x:0.,shift_y:0.,
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
        let object=self.object.to_object(&next)?;
        edit::replace_object(&mut next,self.selected,object)?;
        // The axis still belongs to the form, but the edited objects are
        // stored as RD world coordinates and will survive later translations.
        self.draft=next;
        self.selected=None;
        self.object=ObjectForm::default();
        Ok(())
    }
    fn draw_preview(&self,ui:&mut egui::Ui) {
        let t=self.build().unwrap_or_else(|_|self.draft.clone());
        let (rect,_response)=ui.allocate_exact_size(Vec2::new(ui.available_width().max(240.),190.),
            egui::Sense::hover());
        let painter=ui.painter_at(rect);
        painter.rect_filled(rect,6.,Color32::from_rgb(250,250,250));
        let outline=t.measured_boundary();
        let mut xs=vec![t.start.x,t.end.x];
        let mut ys=vec![t.start.y,t.end.y];
        for &(x,y) in &outline {xs.push(x);ys.push(y);}
        for obj in &t.objects {xs.push(obj.position.x);ys.push(obj.position.y);}
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
            center.y-((y-center_y)*scale) as f32);
        for line in outline.windows(2){
            painter.line_segment([point(line[0].0,line[0].1),point(line[1].0,line[1].1)],
                Stroke::new(2.,Color32::from_rgb(75,85,99)));
        }
        painter.line_segment([point(t.start.x,t.start.y),point(t.end.x,t.end.y)],
            Stroke::new(2.2,ORANGE));
        painter.circle_filled(point(t.start.x,t.start.y),4.,Color32::GREEN);
        painter.circle_filled(point(t.end.x,t.end.y),4.,Color32::RED);
        for (i,obj) in t.objects.iter().enumerate() {
            painter.circle_filled(point(obj.position.x,obj.position.y),
                if Some(i)==self.selected {5.} else {3.},
                if Some(i)==self.selected {ORANGE} else {INK});
        }
        painter.text(rect.left_top()+Vec2::new(8.,6.),egui::Align2::LEFT_TOP,
            format!("Bovenaanzicht | lengte {:.2} m | breedte {:.2} m | {} objecten",
                t.length(),t.width_meters,t.objects.len()),
            egui::FontId::proportional(11.),INK);
    }
}
impl SleufBaseApp {
    pub(super) fn open_virtual_editor(&mut self,index:usize) {
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
        let mut open=true;
        let mut action=None;
        egui::Window::new("Virtuele proefsleuf en meetobjecten bewerken")
            .open(&mut open).default_width(730.)
            .default_height(650.)
            .resizable(true).show(ctx,|ui|{
                egui::ScrollArea::vertical().max_height(660.).show(ui,|ui|{
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
                        if ui.button("Begin- en eindpunt omkeren").clicked() {
                            std::mem::swap(&mut editor.start,&mut editor.end);
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
