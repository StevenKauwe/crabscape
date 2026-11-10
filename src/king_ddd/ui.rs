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
            tab: Tab::Arena,
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
        self.file_name = name.into();
        self.refresh()?;
        if name == "launch-quest.xlsx" {
            self.quest_preset();
        }
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
            .map(|c| {
                previous
                    .iter()
                    .find(|p| p.name == c.name)
                    .cloned()
                    .unwrap_or(c)
            })
            .collect();
        self.request.dataset = dataset;
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
        self.run_rect = None;
        self.buttons.clear();
        if !self.ready {
            theme::set_override(Some(super::theme::royal(false)));
            SheetApp::setup_context(ctx, true);
            self.ready = true;
        }
        self.sheet.logic(ctx);
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
                        RichText::new("KING DATA DRIVEN DECISION")
                            .color(Color32::from_rgb(242, 193, 78)),
                    );
                    ui.label("Ministry of Statistically Defensible Violence");
                    let leave = ui.button("↓ Return to sea");
                    self.buttons.insert("leave".into(), rect_array(leave.rect));
                    if leave.clicked() {
                        actions.push(UiAction::Leave);
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    for (tab, label) in [
                        (Tab::Arena, "⚔ The Lists"),
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
                let panel = if ctx.viewport_rect().width() < 700. {
                    egui::Panel::top("court_controls").default_size(240.)
                } else {
                    egui::Panel::left("court_controls").default_size(260.)
                };
                panel.resizable(true).show(&mut root, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("court_controls_scroll")
                        .show(ui, |ui| self.controls(ui, &mut actions));
                });
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(&mut root, |ui| {
                        let height = (ui.available_height() * 0.48).max(100.);
                        ui.allocate_space(egui::vec2(ui.available_width(), height));
                        egui::Frame::new()
                            .fill(Color32::from_rgba_unmultiplied(10, 15, 25, 235))
                            .inner_margin(12.)
                            .show(ui, |ui| {
                                self.leaderboard(ui, &mut actions);
                            });
                    });
            }
            Tab::Guide => {
                egui::CentralPanel::default().show(&mut root,|ui|{egui::ScrollArea::vertical().show(ui,|ui|{ui.heading("You have reached the surface.");ui.label("Below: the food web. Above: the spreadsheet. The distinction is administrative.");ui.separator();ui.label("1. Open a workbook, or use the royal example. Edit it in the real GridCraft grid.");ui.label("2. Give each column a role. Inputs are choices. Outcomes are learned independently from labelled historical rows. IDs are excluded. Merits are safe arithmetic formulas.");ui.label("3. Limit inputs with ranges, categories, presence rules and overlapping count/sum collections. Choose outcome goal bands or a desired class.");ui.label("4. Summon the Oracle. Candidates are sampled and evolved under your constraints, scored using real local TabICL predictions, ranked, and reaped. No placeholder prediction scores.");ui.label("5. Inspect the survivors and write them back into a new workbook sheet.");ui.separator();ui.label("Court visits use the same separate-scene mechanism as your shell interior. Your crab rests safely at home, and returning or reloading preserves its progress. The kingdom makes no changes to your ocean save.");ui.label("Native edition: optional external LLM assistant and saved fitted-model caches are not included. XLSX/CSV data, constraints, lookups, merits, TabICL and tournaments are local Rust.");});});
            }
        }
        actions
    }
    fn controls(&mut self, ui: &mut egui::Ui, actions: &mut Vec<UiAction>) {
        ui.heading("The Oracle's tournament");
        ui.label("Real TabICL predictions. The lowest-ranked candidates lose their heads. The strongest breed.");
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
            ui.selectable_value(&mut t.aggregate, Aggregate::Product, "All goals (product)");
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
        ui.label("Your data stays on this device. The king only downloads weights.");
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
        if let Some(c) = self.request.columns.get_mut(self.selected) {
            ui.separator();
            ui.heading(&c.name);
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
            if ui.button("Write survivors to workbook").clicked() {
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
                        for v in &c.row {
                            ui.label(if v.is_empty() { "∅" } else { v });
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
        let name = format!("Arena winners {}", self.result.judged);
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
        let mut values = vec![
            headers
                .into_iter()
                .map(serde_json::Value::String)
                .collect::<Vec<_>>(),
        ];
        for (i, c) in self.result.archive.iter().enumerate() {
            let mut row = vec![json!(i + 1), json!(c.hit)];
            row.extend(c.row.iter().map(|v| {
                if v.is_empty() {
                    serde_json::Value::Null
                } else {
                    v.parse::<f64>().ok().map_or_else(|| json!(v), |v| json!(v))
                }
            }));
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
}
