//! The royal workbook is the actual GridCraft spreadsheet, rendered by Bevy's egui integration.
use bevy_egui::egui::{self, Color32, RichText};
use gridcraft_engine::Session;
use gridcraft_ui_egui::{Services, SheetApp, theme};
use king_ddd::{
    Aggregate, Band, BandMode, Collection, Dataset, Generation, Kind, Lookup, Role,
    TournamentConfig, WorkRequest, WorkResult,
};
use serde_json::json;

pub enum UiAction {
    Run(WorkRequest),
    Cancel,
    Leave,
    OpenFile,
    Export { name: String, bytes: Vec<u8> },
}
#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Workbook,
    Rules,
    Arena,
    Guide,
}
pub struct KingUi {
    pub sheet: SheetApp,
    pub request: WorkRequest,
    pub result: WorkResult,
    pub busy: bool,
    pub status: String,
    tab: Tab,
    ready: bool,
    rev: Option<(u64, u64, usize)>,
    pub file_name: String,
    pub selected: usize,
    pub generation: usize,
    data_sheet: usize,
    lookup_sources: std::collections::BTreeMap<String, usize>,
    inspected: usize,
    pub run_rect: Option<[f32; 4]>,
    pub buttons: std::collections::BTreeMap<String, [f32; 4]>,
    input_order: Vec<String>,
}
impl KingUi {
    pub fn new() -> Self {
        let mut sheet = SheetApp::new(Session::new(), Services::default());
        sheet.ui.dark = true;
        let mut this = Self {
            sheet,
            request: WorkRequest {
                dataset: Dataset {
                    headers: vec![],
                    rows: vec![],
                },
                columns: vec![],
                collections: vec![],
                tournament: TournamentConfig::default(),
            },
            result: WorkResult::default(),
            busy: false,
            status: "The ocean made sense. Above it, there are spreadsheets.".into(),
            tab: Tab::Workbook,
            ready: false,
            rev: None,
            file_name: String::new(),
            selected: 0,
            generation: 0,
            data_sheet: 0,
            lookup_sources: Default::default(),
            inspected: 0,
            run_rect: None,
            buttons: Default::default(),
            input_order: vec![],
        };
        if let Err(e) = this.open(
            include_bytes!("../../assets/king-ddd/launch-quest.xlsx"),
            "launch-quest.xlsx",
        ) {
            this.status = e;
        }
        this
    }
    pub fn open(&mut self, bytes: &[u8], name: &str) -> Result<(), String> {
        if bytes.len() > 20_000_000 {
            return Err("Workbook limit: 20 MB".into());
        }
        let mut session = Session::new();
        session
            .run(
                "file.open",
                json!({"name":name,"base64":gridcraft_engine::io::base64_encode(bytes)}),
            )
            .map_err(|e| e.to_string())?;
        self.sheet.session = session;
        self.sheet.views.clear();
        self.sheet.after_engine();
        self.rev = None;
        self.data_sheet = 0;
        self.lookup_sources.clear();
        self.request.columns.clear();
        self.request.collections.clear();
        self.input_order.clear();
        self.file_name = name.into();
        self.refresh()?;
        if name == "launch-quest.xlsx" {
            self.quest_preset();
        }
        self.input_order = self
            .request
            .columns
            .iter()
            .filter(|c| c.role == Role::Input)
            .map(|c| c.name.clone())
            .collect();
        self.result = WorkResult::default();
        Ok(())
    }
    fn dataset(&self, index: usize) -> Result<Dataset, String> {
        let d = self.sheet.session.active().ok_or("No workbook")?;
        let s = d.wb.sheet(index).ok_or("No sheet")?;
        let mut height = 0usize;
        let mut width = 0usize;
        for (p, _) in s.cells.iter() {
            height = height.max(p.row as usize + 1);
            width = width.max(p.col as usize + 1);
        }
        if height > 5001 || width > 64 {
            return Err("Choose a sheet of at most 5,000 data rows and 64 columns".into());
        }
        let mut rows = vec![vec![String::new(); width]; height];
        for (p, c) in s.cells.iter() {
            if let Some(v) = rows
                .get_mut(p.row as usize)
                .and_then(|r| r.get_mut(p.col as usize))
            {
                *v = c.value.display();
            }
        }
        let headers = if rows.is_empty() {
            vec![]
        } else {
            rows.remove(0)
        };
        Ok(Dataset { headers, rows })
    }
    fn refresh(&mut self) -> Result<(), String> {
        let d = self.sheet.session.active().ok_or("No workbook")?;
        let stamp = (d.uid, d.revision, self.data_sheet);
        if self.rev == Some(stamp) {
            return Ok(());
        }
        let dataset = self.dataset(self.data_sheet)?;
        let previous = std::mem::take(&mut self.request.columns);
        self.request.columns = dataset
            .infer_columns()
            .into_iter()
            .enumerate()
            .map(|(i, c)| {
                let mut configured = previous
                    .iter()
                    .find(|p| p.name == c.name)
                    .or_else(|| previous.get(i))
                    .cloned()
                    .unwrap_or_else(|| c.clone());
                if configured.name != c.name
                    && let Some(source) = self.lookup_sources.remove(&configured.name)
                {
                    self.lookup_sources.insert(c.name.clone(), source);
                }
                if configured.name != c.name {
                    for name in &mut self.input_order {
                        if *name == configured.name {
                            *name = c.name.clone();
                        }
                    }
                }
                configured.name = c.name;
                configured
            })
            .collect();
        self.request.dataset = dataset;
        self.sync_input_order();
        let lookups = self
            .lookup_sources
            .iter()
            .filter_map(|(name, index)| {
                self.dataset(*index).ok().map(|table| (name.clone(), table))
            })
            .collect::<Vec<_>>();
        for (name, table) in lookups {
            if let Some(lookup) = self
                .request
                .columns
                .iter_mut()
                .find(|c| c.name == name)
                .and_then(|c| c.lookup.as_mut())
            {
                lookup.table = table;
            }
        }
        self.rev = Some(stamp);
        self.selected = self
            .selected
            .min(self.request.columns.len().saturating_sub(1));
        Ok(())
    }
    fn sync_input_order(&mut self) {
        let now = self
            .request
            .columns
            .iter()
            .filter(|c| c.role == Role::Input)
            .map(|c| c.name.clone())
            .collect::<Vec<_>>();
        if self.input_order != now {
            for group in &mut self.request.collections {
                group.columns = group
                    .columns
                    .iter()
                    .filter_map(|i| {
                        self.input_order
                            .get(*i)
                            .and_then(|name| now.iter().position(|n| n == name))
                    })
                    .collect();
                group.counts.retain(|n| *n <= group.columns.len());
            }
            self.input_order = now;
        }
    }
    fn quest_preset(&mut self) {
        for c in &mut self.request.columns {
            if c.name == "campaign_id" {
                c.role = Role::Id;
            }
            if ["new_customers", "delivery_cost_k", "csat"].contains(&c.name.as_str()) {
                c.role = Role::Output;
            }
        }
        let stores = self
            .sheet
            .session
            .active()
            .and_then(|d| d.wb.sheets.iter().position(|s| s.name == "Stores"))
            .and_then(|i| self.dataset(i).ok());
        if let Some(i) = self
            .sheet
            .session
            .active()
            .and_then(|d| d.wb.sheets.iter().position(|s| s.name == "Stores"))
        {
            self.lookup_sources.insert("store_id".into(), i);
        }
        if let Some(table) = stores {
            let key = table
                .headers
                .iter()
                .position(|n| n == "store_id")
                .unwrap_or(0);
            let fields = (0..table.headers.len()).filter(|i| *i != key).collect();
            if let Some(c) = self
                .request
                .columns
                .iter_mut()
                .find(|c| c.name == "store_id")
            {
                c.lookup = Some(Lookup {
                    table,
                    key,
                    fields,
                    alias: "store".into(),
                });
            }
        }
        let inputs: Vec<_> = self
            .request
            .columns
            .iter()
            .filter(|c| c.role == Role::Input)
            .collect();
        let columns = inputs
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                [
                    "search_spend_k",
                    "social_spend_k",
                    "tv_spend_k",
                    "email_spend_k",
                ]
                .contains(&c.name.as_str())
                .then_some(i)
            })
            .collect::<Vec<_>>();
        if columns.len() == 4 {
            self.request.collections.push(Collection {
                name: "Channel mix".into(),
                columns,
                counts: vec![2, 3],
                sum: Some([20., 90.]),
            });
        }
        if let Some(c) = self
            .request
            .columns
            .iter_mut()
            .find(|c| c.name == "new_customers")
        {
            let [lo, hi] = c
                .domain
                .ranges
                .as_ref()
                .and_then(|r| r.first())
                .copied()
                .unwrap_or([0., 1.]);
            c.goal = Some(Band {
                mode: BandMode::AtLeast,
                lo: lo + (hi - lo) * 0.7,
                hi,
            });
        }
    }
    pub fn raw_input_hook(&mut self, input: &mut egui::RawInput) {
        self.sheet.raw_input_hook(input);
    }
    pub fn accept_generation(&mut self, g: Generation) {
        self.generation = g.generation;
        self.status = format!(
            "Generation {}: {} candidates judged, {} reaped",
            g.generation, g.judged, g.reaped
        );
        self.result.history.push(g);
    }
    pub fn finish(&mut self, result: Result<WorkResult, String>) {
        self.busy = false;
        match result {
            Ok(r) => {
                self.status = format!(
                    "The court has ruled. {} judged; {} reaped; {} distinct survivors.",
                    r.judged,
                    r.reaped,
                    r.archive.len()
                );
                self.result = r;
            }
            Err(e) => self.status = format!("The Oracle refuses: {e}"),
        }
    }
    pub fn show(&mut self, ctx: &egui::Context) -> Vec<UiAction> {
        let was_editing = self.sheet.editor.is_some();
        self.run_rect = None;
        self.buttons.clear();
        if !self.ready {
            theme::set_override(Some(super::theme::royal(false)));
            SheetApp::setup_context(ctx, true);
            self.ready = true;
        }
        self.sheet.logic(ctx);
        // Egui installs a new font set at the next begin_pass. GridCraft's
        // named families must exist before even the court header is painted.
        if !self.sheet.fonts_ready {
            ctx.request_repaint();
            return vec![];
        }
        if let Err(e) = self.refresh() {
            self.status = e;
        }
        let mut actions = vec![];
        let mut root = egui::Ui::new(
            ctx.clone(),
            "king-ddd-root".into(),
            egui::UiBuilder::new()
                .layer_id(egui::LayerId::background())
                .max_rect(ctx.viewport_rect()),
        );
        egui::Panel::top("court_header")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(14, 18, 32))
                    .inner_margin(10.),
            )
            .show(&mut root, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.heading(
                        RichText::new("KING DDD · THE GLOWING LEDGER")
                            .color(Color32::from_rgb(242, 193, 78)),
                    );
                    ui.label("Royal Office of Constrained Possibility");
                    let leave = ui.button("Close ledger");
                    self.buttons.insert("leave".into(), rect_array(leave.rect));
                    if leave.clicked() {
                        actions.push(UiAction::Leave);
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    for (tab, label) in [
                        (Tab::Arena, "Tournament"),
                        (Tab::Workbook, "Workbook"),
                        (Tab::Rules, "Goals & rules"),
                        (Tab::Guide, "Royal decree"),
                    ] {
                        let button = ui.selectable_value(&mut self.tab, tab, label);
                        self.buttons.insert(
                            match tab {
                                Tab::Arena => "arena",
                                Tab::Workbook => "workbook",
                                Tab::Rules => "rules",
                                Tab::Guide => "guide",
                            }
                            .into(),
                            rect_array(button.rect),
                        );
                    }
                    ui.separator();
                    if ui
                        .add_enabled(!self.busy, egui::Button::new("Open XLSX / CSV"))
                        .clicked()
                    {
                        actions.push(UiAction::OpenFile);
                    }
                    if ui.button("Export XLSX").clicked() {
                        match self.export() {
                            Ok(bytes) => actions.push(UiAction::Export {
                                name: format!(
                                    "{}.xlsx",
                                    self.file_name
                                        .rsplit_once('.')
                                        .map(|(stem, _)| stem)
                                        .unwrap_or(&self.file_name)
                                ),
                                bytes,
                            }),
                            Err(e) => self.status = e,
                        }
                    }
                    let example = ui.add_enabled(!self.busy, egui::Button::new("Royal example"));
                    self.buttons
                        .insert("example".into(), rect_array(example.rect));
                    if example.clicked()
                        && let Err(e) = self.open(
                            include_bytes!("../../assets/king-ddd/launch-quest.xlsx"),
                            "launch-quest.xlsx",
                        )
                    {
                        self.status = e;
                    }
                });
            });
        egui::Panel::bottom("court_status").show(&mut root, |ui| {
            ui.label(&self.status);
        });
        match self.tab {
            Tab::Workbook => {
                if self.busy {
                    root.disable();
                }
                self.sheet.ui(&mut root);
            }
            Tab::Rules => {
                egui::CentralPanel::default().show(&mut root, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.add_enabled_ui(!self.busy, |ui| self.rules(ui));
                    });
                });
            }
            Tab::Arena => {
                let phone = ctx.viewport_rect().width() < 700.0;
                let panel = if phone {
                    egui::Panel::top("court_controls_phone").exact_size(250.0)
                } else {
                    egui::Panel::left("court_controls_desktop").default_size(260.0)
                };
                panel.resizable(true).show(&mut root, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(if phone { 230.0 } else { ui.available_height() })
                        .auto_shrink([false, false])
                        .show(ui, |ui| self.controls(ui, &mut actions, phone));
                });
                egui::CentralPanel::default().show(&mut root, |ui| {
                    egui::ScrollArea::both().show(ui, |ui| self.leaderboard(ui, &mut actions));
                });
            }
            Tab::Guide => {
                egui::CentralPanel::default().show(&mut root, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.heading("Above water, the crabs have organized.");
                        ui.label("The starting kelp reaches a hanging rope. Keep climbing it, then step right into the castle. The same walking, jumping and climbing controls work here.");
                        ui.separator();
                        ui.label("This glowing ledger contains the real GridCraft workbook. Open XLSX/CSV, edit the historical data, and give each column its role.");
                        ui.label("Inputs are choices. Outcomes are learned independently from labelled rows. IDs are excluded. Merits are safe arithmetic formulas.");
                        ui.label("Goals & rules sets numeric intervals, categories, presence, lookup features, merit goals and overlapping count/sum collections.");
                        ui.label("The Oracle samples and evolves feasible choices, fits real local TabICL models, ranks their goal probabilities and reaps the weakest. Starting it puts this book down so you can watch the Crabscape puppets in the Lists.");
                        ui.label("Return to the glowing ledger to inspect predictions, write survivors to a new sheet and export XLSX. Closing the book does not stop the tournament; Stop tournament does.");
                        ui.separator();
                        ui.label("The castle is part of the world. Explore its floor and balcony, or descend the rope back to the kelp and sea. Your crab's actual position and progress are saved normally.");
                        ui.label("The workbook and tournament are private to this device. The optional external LLM assistant and persistent model/configuration caches are not included. Export XLSX to keep workbook edits.");
                    });
                });
            }
        }
        if !was_editing && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            actions.push(UiAction::Leave);
        }
        actions
    }
    fn controls(&mut self, ui: &mut egui::Ui, actions: &mut Vec<UiAction>, phone: bool) {
        ui.heading("The Oracle's tournament");
        ui.label("Real TabICL predictions. The lowest-ranked choices are reaped. Crabscape characters show the judged bouts outside this ledger.");
        ui.separator();
        let button = ui.add_enabled(
            !self.busy,
            egui::Button::new(RichText::new("SUMMON THE ORACLE").color(Color32::GOLD)),
        );
        self.run_rect = Some([
            button.rect.min.x,
            button.rect.min.y,
            button.rect.width(),
            button.rect.height(),
        ]);
        if button.clicked() {
            if self.sheet.editor.is_some() && !self.sheet.commit_edit(0, 0, false, false) {
                self.status = "Finish the invalid cell edit before running the Oracle.".into();
                self.tab = Tab::Workbook;
                return;
            }
            self.rev = None;
            if let Err(e) = self.refresh() {
                self.status = e;
                return;
            }
            match self.request.validate() {
                Ok(()) => {
                    self.busy = true;
                    self.result = WorkResult::default();
                    self.status = "Summoning real model weights. This may take a minute…".into();
                    actions.push(UiAction::Run(self.request.clone()));
                }
                Err(e) => {
                    self.status = e;
                    self.tab = Tab::Rules;
                }
            }
        }
        if self.busy {
            ui.spinner();
            if ui.button("Stop tournament").clicked() {
                actions.push(UiAction::Cancel);
                self.busy = false;
                self.status = "Stopping the Oracle. The workbook is unchanged.".into();
            }
        }
        let t = &mut self.request.tournament;
        let short = ui.add_enabled(
            !self.busy,
            egui::Button::new("Short trial: 8 candidates × 2 generations"),
        );
        self.buttons.insert("short".into(), rect_array(short.rect));
        if short.clicked() {
            t.population = 8;
            t.generations = 2;
            t.restarts = 1;
        }
        egui::CollapsingHeader::new("Tournament settings")
            .default_open(!phone)
            .show(ui, |ui| {
                ui.add_enabled_ui(!self.busy, |ui| {
                    ui.add(egui::Slider::new(&mut t.population, 4..=256).text("Population"));
                    ui.add(egui::Slider::new(&mut t.generations, 1..=60).text("Generations"));
                    ui.add(egui::Slider::new(&mut t.restarts, 1..=6).text("Restarts"));
                    ui.add(egui::Slider::new(&mut t.reap, 0.0..=0.95).text("Reap bottom"));
                    ui.add(egui::Slider::new(&mut t.elite, 0.05..=0.8).text("Breed best"));
                    ui.horizontal(|ui| {
                        ui.label("Seed");
                        ui.add(egui::DragValue::new(&mut t.seed));
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.selectable_value(&mut t.aggregate, Aggregate::Mean, "Mean goal chance");
                        ui.selectable_value(
                            &mut t.aggregate,
                            Aggregate::Product,
                            "All goals (product)",
                        );
                    });
                });
            });
        if ui.button("Set goals / constraints").clicked() {
            self.tab = Tab::Rules;
        }
        ui.separator();
        ui.label(format!(
            "{} rows · {} columns",
            self.request.dataset.rows.len(),
            self.request.columns.len()
        ));
        ui.label("Your data stays on this device. Starting the Oracle closes this book so you can watch the Lists.");
    }
    fn export(&mut self) -> Result<Vec<u8>, String> {
        if self.sheet.editor.is_some() && !self.sheet.commit_edit(0, 0, false, false) {
            return Err("Finish the cell edit first".into());
        }
        let value = self
            .sheet
            .session
            .run("file.saveBytes", json!({"format":"xlsx"}))
            .map_err(|e| e.to_string())?;
        value
            .get("base64")
            .and_then(|v| v.as_str())
            .and_then(gridcraft_engine::io::base64_decode)
            .ok_or("Workbook export failed".into())
    }
    fn rules(&mut self, ui: &mut egui::Ui) {
        ui.heading("The laws of the surface");
        let sheets = self
            .sheet
            .session
            .active()
            .map(|d| {
                d.wb.sheets
                    .iter()
                    .map(|s| s.name.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let before = self.data_sheet;
        ui.horizontal_wrapped(|ui| {
            ui.label("Historical data sheet:");
            for (i, name) in sheets.iter().enumerate() {
                ui.selectable_value(&mut self.data_sheet, i, name);
            }
        });
        if self.data_sheet != before {
            self.rev = None;
            self.request.columns.clear();
            self.request.collections.clear();
            self.lookup_sources.clear();
            if let Err(e) = self.refresh() {
                self.status = e;
            }
        }
        ui.label("Click a column to configure it. Blank is absence; zero is a real choice.");
        ui.horizontal_wrapped(|ui| {
            for (i, c) in self.request.columns.iter().enumerate() {
                ui.selectable_value(&mut self.selected, i, format!("{} · {:?}", c.name, c.role));
            }
        });
        if self.request.columns.len() < 64 && ui.button("Add merit formula column").clicked() {
            let mut n = 1;
            while self
                .request
                .columns
                .iter()
                .any(|c| c.name == format!("merit_{n}"))
            {
                n += 1;
            }
            let name = format!("merit_{n}");
            let index = self.request.columns.len();
            let cell = format!("{}1", column_name(index));
            match self.sheet.run(
                "range.setValues",
                json!({"sheet":self.data_sheet,"range":cell,"values":[[name]]}),
            ) {
                Ok(_) => {
                    self.rev = None;
                    if let Err(e) = self.refresh() {
                        self.status = e;
                    } else {
                        let source = self
                            .request
                            .columns
                            .iter()
                            .find(|c| {
                                c.kind == Kind::Number
                                    && matches!(c.role, Role::Input | Role::Output)
                            })
                            .map(|c| format!("[{}]", c.name))
                            .unwrap_or_else(|| "0".into());
                        if let Some(c) = self.request.columns.get_mut(index) {
                            c.role = Role::Merit;
                            c.formula = source;
                        }
                        self.selected = index;
                    }
                }
                Err(e) => self.status = e.to_string(),
            }
        }
        let lookup_sheets: Vec<_> = self
            .sheet
            .session
            .active()
            .map(|d| {
                d.wb.sheets
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != self.data_sheet)
                    .map(|(i, s)| (i, s.name.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let mut lookup_choice = None;
        let mut recalculate_merit = false;
        let old_kind = self
            .request
            .columns
            .get(self.selected)
            .map(|c| c.kind.clone());
        if let Some(c) = self.request.columns.get_mut(self.selected) {
            ui.separator();
            ui.heading(&c.name);
            if c.role != Role::Merit {
                ui.horizontal(|ui| {
                    ui.label("Values");
                    ui.selectable_value(&mut c.kind, Kind::Number, "Numbers");
                    ui.selectable_value(&mut c.kind, Kind::Category, "Categories");
                });
            }
            ui.horizontal_wrapped(|ui| {
                for (role, label) in [
                    (Role::Input, "Input"),
                    (Role::Output, "Outcome"),
                    (Role::Merit, "Merit formula"),
                    (Role::Id, "ID"),
                    (Role::Ignore, "Ignore"),
                ] {
                    ui.selectable_value(&mut c.role, role, label);
                }
            });
            match c.role {
                Role::Input => {
                    ui.horizontal(|ui| {
                        ui.label("Presence");
                        for p in ["required", "optional", "empty"] {
                            ui.selectable_value(&mut c.domain.presence, p.into(), p);
                        }
                    });
                    if c.kind == Kind::Number {
                        ui.label("Allowed numeric intervals (inclusive)");
                        let ranges = c.domain.ranges.get_or_insert_with(|| vec![[0., 1.]]);
                        let mut remove = None;
                        for (i, r) in ranges.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                ui.add(egui::DragValue::new(&mut r[0]).speed(0.1));
                                ui.label("to");
                                ui.add(egui::DragValue::new(&mut r[1]).speed(0.1));
                                if ui.small_button("×").clicked() {
                                    remove = Some(i);
                                }
                            });
                        }
                        if let Some(i) = remove {
                            ranges.remove(i);
                        }
                        if ranges.len() < 16 && ui.button("Add interval").clicked() {
                            ranges.push([0., 1.]);
                        }
                    } else {
                        let mut text = c.domain.classes.clone().unwrap_or_default().join("\n");
                        ui.label("Allowed categories, one per line");
                        if ui.text_edit_multiline(&mut text).changed() {
                            c.domain.classes = Some(
                                text.lines()
                                    .filter(|s| !s.is_empty())
                                    .map(str::to_string)
                                    .collect(),
                            );
                        }
                    }
                    ui.label("Lookup features from another sheet");
                    if ui.button("No lookup").clicked() {
                        c.lookup = None;
                    }
                    for (i, name) in &lookup_sheets {
                        if ui.button(format!("Link {name}")).clicked() {
                            lookup_choice = Some(*i);
                        }
                    }
                    if let Some(l) = &mut c.lookup {
                        ui.horizontal(|ui| {
                            ui.label("Feature prefix");
                            ui.text_edit_singleline(&mut l.alias);
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Key");
                            for (i, n) in l.table.headers.iter().enumerate() {
                                ui.selectable_value(&mut l.key, i, n);
                            }
                        });
                        ui.label("Model features");
                        for (i, n) in l
                            .table
                            .headers
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| *i != l.key)
                        {
                            let mut included = l.fields.contains(&i);
                            if ui.checkbox(&mut included, n).changed() {
                                if included {
                                    l.fields.push(i);
                                } else {
                                    l.fields.retain(|v| *v != i);
                                }
                            }
                        }
                    }
                }
                Role::Output | Role::Merit => {
                    if c.role == Role::Merit {
                        ui.label("Formula: [column] + - * /, abs, min, max, coalesce");
                        ui.text_edit_singleline(&mut c.formula);
                        recalculate_merit = ui
                            .button("Calculate historical values in this column")
                            .clicked();
                    }
                    if c.kind == Kind::Category && c.role == Role::Output {
                        let mut desired = c.desired_class.clone().unwrap_or_default();
                        ui.label("Desired class (blank = no goal)");
                        if ui.text_edit_singleline(&mut desired).changed() {
                            c.desired_class = (!desired.is_empty()).then_some(desired);
                        }
                    } else {
                        let mut enabled = c.goal.is_some();
                        if ui.checkbox(&mut enabled, "Include this goal").changed() {
                            c.goal = enabled.then_some(Band {
                                mode: BandMode::AtLeast,
                                lo: 0.,
                                hi: 1.,
                            });
                        }
                        if let Some(b) = &mut c.goal {
                            band_ui(ui, b);
                        }
                    }
                }
                _ => {
                    ui.label("This column will not be used as a model feature.");
                }
            }
        }
        if let Some(old_kind) = old_kind
            && self
                .request
                .columns
                .get(self.selected)
                .is_some_and(|c| c.kind != old_kind)
        {
            self.change_kind(self.selected, old_kind);
        }
        if recalculate_merit {
            match self.write_merit_history(self.selected) {
                Ok(()) => {
                    self.status =
                        "Historical merit values calculated. Missing inputs stay empty.".into()
                }
                Err(e) => self.status = e,
            }
        }
        if let Some(i) = lookup_choice
            && let Ok(table) = self.dataset(i)
        {
            let key = table
                .headers
                .iter()
                .position(|h| Some(h) == self.request.columns.get(self.selected).map(|c| &c.name))
                .unwrap_or(0);
            let fields = (0..table.headers.len()).filter(|i| *i != key).collect();
            if let Some(c) = self.request.columns.get_mut(self.selected) {
                self.lookup_sources.insert(c.name.clone(), i);
                c.lookup = Some(Lookup {
                    table,
                    key,
                    fields,
                    alias: c.name.clone(),
                });
            }
        }
        self.sync_input_order();
        ui.separator();
        ui.heading("Collections: choices that must obey one law");
        let input_names: Vec<_> = self
            .request
            .columns
            .iter()
            .filter(|c| c.role == Role::Input)
            .map(|c| c.name.clone())
            .collect();
        let mut remove = None;
        for (i, g) in self.request.collections.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut g.name);
                        if ui.button("Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        for (j, name) in input_names.iter().enumerate() {
                            let mut on = g.columns.contains(&j);
                            if ui.checkbox(&mut on, name).changed() {
                                if on {
                                    g.columns.push(j);
                                } else {
                                    g.columns.retain(|i| *i != j);
                                }
                            }
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Allowed present counts");
                        for n in 0..=g.columns.len() {
                            let mut on = g.counts.contains(&n);
                            if ui.checkbox(&mut on, n.to_string()).changed() {
                                if on {
                                    g.counts.push(n);
                                } else {
                                    g.counts.retain(|v| *v != n);
                                }
                            }
                        }
                    });
                    let mut sum = g.sum.is_some();
                    if ui.checkbox(&mut sum, "Constrain numeric sum").changed() {
                        g.sum = sum.then_some([0., 100.]);
                    }
                    if let Some([lo, hi]) = &mut g.sum {
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(lo));
                            ui.label("≤ sum ≤");
                            ui.add(egui::DragValue::new(hi));
                        });
                    }
                });
            });
        }
        if let Some(i) = remove {
            self.request.collections.remove(i);
        }
        if self.request.collections.len() < 12 && ui.button("Add collection").clicked() {
            self.request.collections.push(Collection {
                name: "Royal budget".into(),
                columns: vec![],
                counts: vec![0],
                sum: None,
            });
        }
        if ui.button("Check feasible choices").clicked() {
            self.status = match king_ddd::sample_result(
                &self.request.constraint_request(),
                1,
                self.request.tournament.seed,
            ) {
                Ok(_) => "A valid choice exists. The king permits the tournament.".into(),
                Err(e) => e,
            };
        }
    }
    fn leaderboard(&mut self, ui: &mut egui::Ui, _actions: &mut Vec<UiAction>) {
        ui.heading("The survivors");
        if self.result.archive.is_empty() {
            ui.label("The royal example is ready. Summon the Oracle to begin.");
        } else {
            let winners = ui.button("Write survivors to workbook");
            self.buttons
                .insert("winners".into(), rect_array(winners.rect));
            if winners.clicked() {
                if let Err(e) = self.write_winners() {
                    self.status = e;
                } else {
                    self.tab = Tab::Workbook;
                }
            }
            egui::ScrollArea::both().max_height(260.).show(ui, |ui| {
                egui::Grid::new("survivors").striped(true).show(ui, |ui| {
                    ui.strong("Rank");
                    ui.strong("Goal chance");
                    for c in self
                        .request
                        .columns
                        .iter()
                        .filter(|c| c.role == Role::Input)
                    {
                        ui.strong(&c.name);
                    }
                    ui.end_row();
                    for (i, c) in self.result.archive.iter().enumerate() {
                        ui.selectable_value(&mut self.inspected, i, format!("#{}", i + 1));
                        ui.label(
                            c.hit
                                .map(|v| format!("{:.2}%", v * 100.))
                                .unwrap_or_else(|| "—".into()),
                        );
                        for (v, column) in c.row.iter().zip(
                            self.request
                                .columns
                                .iter()
                                .filter(|c| c.role == Role::Input),
                        ) {
                            ui.label(if v.is_empty() {
                                "(empty)".into()
                            } else if column.kind == Kind::Number {
                                v.parse::<f64>()
                                    .map(format_number)
                                    .unwrap_or_else(|_| v.clone())
                            } else {
                                v.clone()
                            });
                        }
                        ui.end_row();
                    }
                });
            });
            if let Some(candidate) = self.result.archive.get(self.inspected) {
                ui.collapsing(format!("Inspect survivor #{}", self.inspected + 1), |ui| {
                    for (name, prediction) in &candidate.predictions {
                        match prediction {
                            king_ddd::Prediction::Numeric { mean, median, q05, q95 } => { ui.label(format!("{name}: median {median:.4} · mean {mean:.4} · 90% band [{q05:.4}, {q95:.4}]")); }
                            king_ddd::Prediction::Categorical { labels, proba } => { for (label,p) in labels.iter().zip(proba) { ui.label(format!("{name} = {label}: {:.2}%", p * 100.)); } }
                        }
                    }
                    for (name, value) in &candidate.merits { ui.label(format!("{name}: {}", value.map(|v|format!("{v:.4}")).unwrap_or_else(||"missing".into()))); }
                });
            }
        }
        if let Some(g) = self.result.history.last() {
            ui.label(format!(
                "Generation {} · {} alive · {} reaped",
                g.generation, g.alive, g.reaped
            ));
        }
    }
    fn write_winners(&mut self) -> Result<(), String> {
        let base = format!("Arena winners {}", self.result.judged);
        let names = self
            .sheet
            .session
            .active()
            .map(|d| {
                d.wb.sheets
                    .iter()
                    .map(|s| s.name.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut name = base.clone();
        let mut suffix = 2;
        while names.contains(&name) {
            name = format!("{base} ({suffix})");
            suffix += 1;
        }
        let sheets = self
            .sheet
            .session
            .active()
            .map(|d| d.wb.sheets.len())
            .unwrap_or(0);
        self.sheet
            .run("home.insertSheet", json!({"name":name,"before":sheets}))
            .map_err(|e| e.to_string())?;
        let mut headers = vec!["rank".to_string(), "goal_chance".into()];
        headers.extend(
            self.request
                .columns
                .iter()
                .filter(|c| c.role == Role::Input)
                .map(|c| c.name.clone()),
        );
        let outputs = self
            .request
            .columns
            .iter()
            .filter(|c| c.role == Role::Output)
            .map(|c| (c.name.clone(), c.kind.clone()))
            .collect::<Vec<_>>();
        let merits = self
            .request
            .columns
            .iter()
            .filter(|c| c.role == Role::Merit)
            .map(|c| c.name.clone())
            .collect::<Vec<_>>();
        for (name, kind) in &outputs {
            headers.push(format!("{name} (pred)"));
            if *kind == Kind::Number {
                headers.push(format!("{name} q05"));
                headers.push(format!("{name} q95"));
            }
        }
        headers.extend(merits.iter().cloned());
        let mut values = vec![
            headers
                .into_iter()
                .map(|text| json!(format!("'{text}")))
                .collect::<Vec<_>>(),
        ];
        for (i, c) in self.result.archive.iter().enumerate() {
            let mut row = vec![json!(i + 1), json!(c.hit)];
            row.extend(
                c.row
                    .iter()
                    .zip(
                        self.request
                            .columns
                            .iter()
                            .filter(|c| c.role == Role::Input),
                    )
                    .map(|(v, column)| {
                        if v.is_empty() {
                            serde_json::Value::Null
                        } else {
                            if column.kind == Kind::Number {
                                v.parse::<f64>()
                                    .ok()
                                    .map_or_else(|| json!(format!("'{v}")), |v| json!(v))
                            } else {
                                json!(format!("'{v}"))
                            }
                        }
                    }),
            );
            for (name, kind) in &outputs {
                match (kind, c.predictions.get(name)) {
                    (
                        Kind::Number,
                        Some(king_ddd::Prediction::Numeric {
                            median, q05, q95, ..
                        }),
                    ) => row.extend([json!(median), json!(q05), json!(q95)]),
                    (Kind::Category, Some(king_ddd::Prediction::Categorical { labels, proba })) => {
                        let label = proba
                            .iter()
                            .enumerate()
                            .max_by(|(_, a), (_, b)| a.total_cmp(b))
                            .and_then(|(i, _)| labels.get(i));
                        row.push(json!(label.map(|v| format!("'{v}"))));
                    }
                    (Kind::Number, _) => row.extend([const { serde_json::Value::Null }; 3]),
                    _ => row.push(serde_json::Value::Null),
                }
            }
            for name in &merits {
                row.push(json!(c.merits.get(name).copied().flatten()));
            }
            values.push(row);
        }
        self.sheet
            .run(
                "range.setValues",
                json!({"sheet":name,"range":"A1","values":values}),
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn change_kind(&mut self, index: usize, old: Kind) {
        let values = self
            .request
            .dataset
            .rows
            .iter()
            .filter_map(|r| r.get(index))
            .filter(|s| !s.trim().is_empty())
            .cloned()
            .collect::<Vec<_>>();
        if let Some(c) = self.request.columns.get_mut(index) {
            if c.kind == Kind::Category {
                c.domain.ranges = None;
                c.domain.classes = Some(
                    values
                        .into_iter()
                        .collect::<std::collections::BTreeSet<_>>()
                        .into_iter()
                        .collect(),
                );
            } else if let Some(numbers) = values
                .iter()
                .map(|v| v.parse::<f64>().ok().filter(|v| v.is_finite()))
                .collect::<Option<Vec<_>>>()
            {
                c.domain.classes = None;
                c.domain.ranges = Some(vec![[
                    numbers.iter().copied().reduce(f64::min).unwrap_or(0.),
                    numbers.iter().copied().reduce(f64::max).unwrap_or(1.),
                ]]);
            } else {
                c.kind = old;
                self.status="This column contains non-numeric values. Keep it categorical or edit those cells first.".into();
                return;
            }
            c.goal = None;
            c.desired_class = None;
        }
    }
    fn write_merit_history(&mut self, index: usize) -> Result<(), String> {
        let column = self.request.columns.get(index).ok_or("No merit column")?;
        let names = self
            .request
            .columns
            .iter()
            .filter(|c| c.kind == Kind::Number && matches!(c.role, Role::Input | Role::Output))
            .map(|c| c.name.clone())
            .collect::<Vec<_>>();
        let tree = king_ddd::compile_formula(&column.formula, &names)?;
        let values = self
            .request
            .dataset
            .rows
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let map = self
                    .request
                    .columns
                    .iter()
                    .zip(row)
                    .map(|(c, v)| (c.name.clone(), json!(v)))
                    .collect::<serde_json::Map<_, _>>();
                king_ddd::constraints::eval_node(&tree, &serde_json::Value::Object(map), 0, &mut 0)
                    .map(|v| vec![json!(v)])
                    .map_err(|e| format!("Row {}: {e}", i + 2))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if !values.is_empty() {
            self.sheet.run("range.setValues",json!({"sheet":self.data_sheet,"range":format!("{}2",column_name(index)),"values":values})).map_err(|e|e.to_string())?;
        }
        self.rev = None;
        self.refresh()
    }
}
fn band_ui(ui: &mut egui::Ui, b: &mut Band) {
    ui.horizontal_wrapped(|ui| {
        for (mode, label) in [
            (BandMode::AtMost, "At most"),
            (BandMode::Between, "Between"),
            (BandMode::Outside, "Outside"),
            (BandMode::AtLeast, "At least"),
        ] {
            ui.selectable_value(&mut b.mode, mode, label);
        }
    });
    ui.horizontal(|ui| {
        if b.mode != BandMode::AtMost {
            ui.add(egui::DragValue::new(&mut b.lo).speed(0.1));
        }
        if matches!(b.mode, BandMode::Between | BandMode::Outside) {
            ui.label("and");
        }
        if b.mode != BandMode::AtLeast {
            ui.add(egui::DragValue::new(&mut b.hi).speed(0.1));
        }
    });
}
fn rect_array(r: egui::Rect) -> [f32; 4] {
    [r.min.x, r.min.y, r.width(), r.height()]
}
fn format_number(value: f64) -> String {
    if value.abs() >= 1e6 || (value != 0. && value.abs() < 0.001) {
        format!("{value:.3e}")
    } else {
        format!("{value:.4}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}
fn column_name(mut index: usize) -> String {
    let mut chars = vec![];
    loop {
        chars.push((b'A' + (index % 26) as u8) as char);
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    chars.into_iter().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sample_is_real_and_ready() {
        let ui = KingUi::new();
        assert!(!ui.request.dataset.rows.is_empty());
        assert_eq!(
            ui.request
                .columns
                .iter()
                .filter(|c| c.role == Role::Output)
                .count(),
            3
        );
        assert_eq!(ui.request.collections.len(), 1);
        ui.request.validate().unwrap();
    }
    #[test]
    fn viewing_another_sheet_does_not_change_training_data() {
        let mut ui = KingUi::new();
        let before = ui.request.dataset.headers.clone();
        let index = ui
            .sheet
            .session
            .active()
            .unwrap()
            .wb
            .sheets
            .iter()
            .position(|s| s.name == "Stores")
            .unwrap();
        ui.sheet
            .run("sheet.activate", json!({"index":index}))
            .unwrap();
        ui.refresh().unwrap();
        assert_eq!(before, ui.request.dataset.headers);
        assert_eq!(
            ui.request
                .columns
                .iter()
                .filter(|c| c.role == Role::Output)
                .count(),
            3
        );
    }
    #[test]
    fn export_preserves_workbook_sheets() {
        let mut ui = KingUi::new();
        let bytes = ui.export().unwrap();
        let book = king_ddd::Book::new(&bytes, true).unwrap();
        let value: serde_json::Value = serde_json::from_str(&book.snapshot(0).unwrap()).unwrap();
        assert!(
            value["sheets"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "Stores")
        );
    }
    #[test]
    fn spreadsheet_column_names() {
        assert_eq!(column_name(0), "A");
        assert_eq!(column_name(26), "AA");
        assert_eq!(column_name(63), "BL");
    }
    #[test]
    fn royal_fonts_are_installed_before_painting() {
        let mut ui = KingUi::new();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            ctx.begin_pass(egui::RawInput::default());
            let _ = ui.show(&ctx);
            let mut output = ctx.end_pass();
            output.textures_delta.clear();
        }
        assert!(ui.sheet.fonts_ready);
    }
    #[test]
    fn input_roles_do_not_retarget_collection_members() {
        let mut ui = KingUi::new();
        let names = |ui: &KingUi| {
            ui.request.collections[0]
                .columns
                .iter()
                .map(|i| ui.input_order[*i].clone())
                .collect::<Vec<_>>()
        };
        let before = names(&ui);
        ui.request
            .columns
            .iter_mut()
            .find(|c| c.name == "store_id")
            .unwrap()
            .role = Role::Ignore;
        ui.sync_input_order();
        assert_eq!(before, names(&ui));
    }
}

#[cfg(test)]
mod roundtrip_tests {
    use super::*;
    #[test]
    fn numeric_labels_can_be_classified() {
        let mut ui = KingUi::new();
        let i = ui
            .request
            .columns
            .iter()
            .position(|c| c.name == "new_customers")
            .unwrap();
        ui.request.columns[i].kind = Kind::Category;
        ui.change_kind(i, Kind::Number);
        assert!(ui.request.columns[i].domain.ranges.is_none());
        assert!(
            !ui.request.columns[i]
                .domain
                .classes
                .as_ref()
                .unwrap()
                .is_empty()
        );
        assert!(ui.request.columns[i].goal.is_none());
    }
    #[test]
    fn merit_history_is_written_by_real_grid_commands() {
        let mut ui = KingUi::new();
        let n = ui.request.columns.len();
        ui.sheet
            .run(
                "range.setValues",
                json!({"sheet":0,"range":format!("{}1",column_name(n)),"values":[["merit_test"]]}),
            )
            .unwrap();
        ui.rev = None;
        ui.refresh().unwrap();
        ui.request.columns[n].role = Role::Merit;
        ui.request.columns[n].formula = "[new_customers] * 2".into();
        let source = ui
            .request
            .columns
            .iter()
            .position(|c| c.name == "new_customers")
            .unwrap();
        let expected = ui.request.dataset.rows[0][source].parse::<f64>().unwrap() * 2.;
        ui.write_merit_history(n).unwrap();
        assert_eq!(
            ui.request.dataset.rows[0][n].parse::<f64>().unwrap(),
            expected
        );
    }
    #[test]
    fn repeated_winner_exports_keep_predictions_and_text_ids() {
        let mut ui = KingUi::new();
        let mut row = ui
            .request
            .columns
            .iter()
            .enumerate()
            .filter(|(_, c)| c.role == Role::Input)
            .map(|(i, _)| ui.request.dataset.rows[0][i].clone())
            .collect::<Vec<_>>();
        row[0] = "001".into();
        let predictions = ui
            .request
            .columns
            .iter()
            .filter(|c| c.role == Role::Output)
            .map(|c| {
                (
                    c.name.clone(),
                    king_ddd::Prediction::Numeric {
                        mean: 3.,
                        median: 2.,
                        q05: 1.,
                        q95: 4.,
                    },
                )
            })
            .collect();
        ui.result = WorkResult {
            archive: vec![king_ddd::Candidate {
                row,
                predictions,
                merits: Default::default(),
                hit: Some(0.5),
                reaped: None,
                generation: 1,
            }],
            judged: 1,
            ..Default::default()
        };
        ui.write_winners().unwrap();
        ui.write_winners().unwrap();
        let doc = ui.sheet.session.active().unwrap();
        assert!(
            doc.wb
                .sheets
                .iter()
                .any(|s| s.name == "Arena winners 1 (2)")
        );
        let data = ui.dataset(doc.wb.sheets.len() - 1).unwrap();
        assert_eq!(data.rows[0][2], "001");
        let col = data
            .headers
            .iter()
            .position(|h| h == "new_customers q05")
            .unwrap();
        assert_eq!(data.rows[0][col], "1");
    }
}
