//! Native decision arena. No network, browser, clocks or fallback prediction scores.
pub mod constraints;
mod formula;
mod oracle;
pub use constraints::{Book, Collection, Domain, Request, evolve_result, sample_result};
pub use formula::compile_formula;
pub use oracle::Oracle;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Role {
    Input,
    Output,
    Merit,
    Id,
    Ignore,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Kind {
    Number,
    Category,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum BandMode {
    AtMost,
    Between,
    Outside,
    AtLeast,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Band {
    pub mode: BandMode,
    pub lo: f64,
    pub hi: f64,
}
impl Band {
    pub fn hit(&self, p: impl Fn(f64, f64) -> Result<f64, String>) -> Result<f64, String> {
        let (lo, hi) = (self.lo.min(self.hi), self.lo.max(self.hi));
        if !lo.is_finite() || !hi.is_finite() {
            return Err("Nonfinite goal".into());
        }
        match self.mode {
            BandMode::AtMost => p(-1e12, hi),
            BandMode::AtLeast => p(lo, 1e12),
            BandMode::Outside => Ok(1. - p(lo, hi)?),
            BandMode::Between => p(lo, hi),
        }
    }
    pub fn ranges(&self, min: f64, max: f64) -> Vec<[f64; 2]> {
        let (lo, hi) = (self.lo.min(self.hi), self.lo.max(self.hi));
        match self.mode {
            BandMode::AtMost => vec![[min, hi.max(min)]],
            BandMode::AtLeast => vec![[lo.min(max), max]],
            BandMode::Between => vec![[lo, hi]],
            BandMode::Outside => vec![[min, lo], [hi, max]]
                .into_iter()
                .filter(|[a, b]| b >= a && !(*a == lo && *b == hi))
                .collect(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Dataset {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lookup {
    pub table: Dataset,
    pub key: usize,
    pub fields: Vec<usize>,
    pub alias: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    pub role: Role,
    pub kind: Kind,
    pub domain: Domain,
    pub goal: Option<Band>,
    pub desired_class: Option<String>,
    pub formula: String,
    pub lookup: Option<Lookup>,
}
impl Dataset {
    pub fn infer_columns(&self) -> Vec<Column> {
        self.headers
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let vals: Vec<_> = self
                    .rows
                    .iter()
                    .map(|r| r.get(i).map(String::as_str).unwrap_or(""))
                    .collect();
                let nums: Option<Vec<f64>> = vals
                    .iter()
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| s.parse::<f64>().ok().filter(|v| v.is_finite()))
                    .collect();
                let numeric = nums.is_some();
                let ranges = nums.map(|v| {
                    vec![[
                        v.iter().copied().reduce(f64::min).unwrap_or(0.),
                        v.iter().copied().reduce(f64::max).unwrap_or(1.),
                    ]]
                });
                Column {
                    name: name.clone(),
                    role: if name.trim().is_empty() {
                        Role::Ignore
                    } else {
                        Role::Input
                    },
                    kind: if numeric {
                        Kind::Number
                    } else {
                        Kind::Category
                    },
                    domain: Domain {
                        ranges,
                        classes: (!numeric).then(|| {
                            vals.iter()
                                .filter(|s| !s.is_empty())
                                .map(|s| s.to_string())
                                .collect::<BTreeSet<_>>()
                                .into_iter()
                                .collect()
                        }),
                        presence: if vals.iter().any(|s| s.trim().is_empty()) {
                            "optional"
                        } else {
                            "required"
                        }
                        .into(),
                    },
                    goal: None,
                    desired_class: None,
                    formula: String::new(),
                    lookup: None,
                }
            })
            .collect()
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Aggregate {
    Mean,
    Product,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TournamentConfig {
    pub population: usize,
    pub generations: usize,
    pub restarts: usize,
    pub elite: f64,
    pub reap: f64,
    pub seed: u32,
    pub aggregate: Aggregate,
}
impl Default for TournamentConfig {
    fn default() -> Self {
        Self {
            population: 48,
            generations: 10,
            restarts: 1,
            elite: 0.25,
            reap: 0.5,
            seed: 7,
            aggregate: Aggregate::Mean,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkRequest {
    pub dataset: Dataset,
    pub columns: Vec<Column>,
    pub collections: Vec<Collection>,
    pub tournament: TournamentConfig,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Prediction {
    Numeric {
        mean: f64,
        median: f64,
        q05: f64,
        q95: f64,
    },
    Categorical {
        labels: Vec<String>,
        proba: Vec<f64>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub row: Vec<String>,
    pub predictions: BTreeMap<String, Prediction>,
    pub merits: BTreeMap<String, Option<f64>>,
    pub hit: Option<f64>,
    pub reaped: Option<String>,
    pub generation: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Generation {
    pub generation: usize,
    pub best: Option<f64>,
    pub median: Option<f64>,
    pub alive: usize,
    pub reaped: usize,
    pub judged: usize,
    /// Actual selected candidates for the court's bouts, in rank order.
    pub champions: Vec<Candidate>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WorkResult {
    pub archive: Vec<Candidate>,
    pub history: Vec<Generation>,
    pub judged: usize,
    pub reaped: usize,
}
impl WorkRequest {
    pub fn constraint_request(&self) -> Request {
        Request {
            domains: self
                .columns
                .iter()
                .filter(|c| c.role == Role::Input)
                .map(|c| c.domain.clone())
                .collect(),
            collections: self.collections.clone(),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.dataset.headers.len() != self.columns.len()
            || self.columns.len() > 64
            || self.dataset.rows.len() > 5000
        {
            return Err(
                "Table/configuration width mismatch or table exceeds 5,000 rows / 64 columns"
                    .into(),
            );
        }
        let mut names = BTreeSet::new();
        for (h, c) in self.dataset.headers.iter().zip(&self.columns) {
            if h != &c.name {
                return Err("Column names must match the table".into());
            }
            if c.role != Role::Ignore && !names.insert(&c.name) {
                return Err("Column names must be unique".into());
            }
        }
        let t = &self.tournament;
        if !(1..=256).contains(&t.population)
            || !(1..=60).contains(&t.generations)
            || !(1..=6).contains(&t.restarts)
            || !t.reap.is_finite()
            || !(0.0..1.).contains(&t.reap)
            || !t.elite.is_finite()
            || !(0.0..=1.).contains(&t.elite)
        {
            return Err("Invalid tournament settings".into());
        }
        if !self.columns.iter().any(|c| c.role == Role::Output) {
            return Err("Choose an outcome".into());
        }
        if !self.columns.iter().any(|c| {
            matches!(c.role, Role::Output | Role::Merit)
                && (c.goal.is_some() || c.desired_class.is_some())
        }) {
            return Err("Set an outcome or merit goal".into());
        }
        Ok(())
    }
}
pub fn score_candidates(
    req: &WorkRequest,
    rows: Vec<Vec<String>>,
    predictions: BTreeMap<String, Vec<Prediction>>,
    generation: usize,
) -> Result<Vec<Candidate>, String> {
    let inputs: Vec<_> = req
        .columns
        .iter()
        .filter(|c| c.role == Role::Input)
        .collect();
    let mut formulas = BTreeMap::new();
    let numeric: Vec<_> = req
        .columns
        .iter()
        .filter(|c| c.kind == Kind::Number && matches!(c.role, Role::Input | Role::Output))
        .map(|c| c.name.clone())
        .collect();
    for c in &req.columns {
        if c.role == Role::Merit {
            formulas.insert(c.name.clone(), compile_formula(&c.formula, &numeric)?);
        }
    }
    rows.into_iter()
        .enumerate()
        .map(|(i, row)| {
            let mut values = serde_json::Map::new();
            for (c, v) in inputs.iter().zip(&row) {
                values.insert(c.name.clone(), serde_json::Value::String(v.clone()));
            }
            let mut preds = BTreeMap::new();
            let mut hits = vec![];
            let mut reaped = None;
            for c in req.columns.iter().filter(|c| c.role == Role::Output) {
                let p = predictions
                    .get(&c.name)
                    .and_then(|p| p.get(i))
                    .ok_or_else(|| format!("Missing prediction for {}", c.name))?
                    .clone();
                match &p {
                    Prediction::Numeric {
                        median, q05, q95, ..
                    } => {
                        values.insert(c.name.clone(), serde_json::json!(median));
                        if let Some(b) = &c.goal {
                            match b.hit(|a, z| {
                                constraints::interval_probability(*q05, *median, *q95, a, z)
                            }) {
                                Ok(p) => hits.push(p),
                                Err(_) => reaped = Some("degenerate prediction band".into()),
                            }
                        }
                    }
                    Prediction::Categorical { labels, proba } => {
                        if let Some(want) = &c.desired_class {
                            hits.push(
                                labels
                                    .iter()
                                    .position(|x| x == want)
                                    .map(|j| proba[j])
                                    .unwrap_or(0.),
                            );
                        }
                    }
                }
                preds.insert(c.name.clone(), p);
            }
            let values = serde_json::Value::Object(values);
            let mut merits = BTreeMap::new();
            for c in req.columns.iter().filter(|c| c.role == Role::Merit) {
                let value = constraints::eval_node(&formulas[&c.name], &values, 0, &mut 0)
                    .ok()
                    .flatten();
                if let Some(b) = &c.goal {
                    hits.push(match value {
                        Some(v) => b.hit(|lo, hi| Ok(if v >= lo && v <= hi { 1. } else { 0. }))?,
                        None => 0.,
                    });
                }
                merits.insert(c.name.clone(), value);
            }
            let hit = if hits.is_empty() {
                reaped = Some("no goals set".into());
                None
            } else if hits
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.).contains(v))
            {
                reaped = Some("invalid probability".into());
                None
            } else {
                Some(match req.tournament.aggregate {
                    Aggregate::Mean => hits.iter().sum::<f64>() / hits.len() as f64,
                    Aggregate::Product => hits.iter().product(),
                })
            };
            Ok(Candidate {
                row,
                predictions: preds,
                merits,
                hit,
                reaped,
                generation,
            })
        })
        .collect()
}
/// Each callback is a completed generation. The caller owns cancellation/thread scheduling.
pub fn run(
    req: &WorkRequest,
    regressor_weights: Option<Vec<u8>>,
    classifier_weights: Option<Vec<u8>>,
    mut progress: impl FnMut(&Generation),
) -> Result<WorkResult, String> {
    run_cancellable(
        req,
        regressor_weights,
        classifier_weights,
        &mut progress,
        || false,
    )
}
pub fn run_cancellable(
    req: &WorkRequest,
    regressor_weights: Option<Vec<u8>>,
    classifier_weights: Option<Vec<u8>>,
    mut progress: impl FnMut(&Generation),
    cancelled: impl Fn() -> bool,
) -> Result<WorkResult, String> {
    req.validate()?;
    if cancelled() {
        return Err("Tournament cancelled".into());
    }
    let mut oracle = Oracle::new(regressor_weights, classifier_weights);
    oracle.fit_cancellable(req, &cancelled)?;
    let constraints = req.constraint_request();
    let t = &req.tournament;
    let mut result = WorkResult::default();
    for restart in 0..t.restarts {
        let mut parents = vec![];
        for g in 0..t.generations {
            if cancelled() {
                return Err("Tournament cancelled".into());
            }
            let seed = t
                .seed
                .wrapping_mul(7919)
                .wrapping_add(restart as u32 * 1009)
                .wrapping_add(g as u32 * 31);
            let rows = if parents.is_empty() {
                sample_result(&constraints, t.population, seed)?.rows
            } else {
                evolve_result(&constraints, &parents, t.population, seed)?.rows
            };
            let preds = oracle.predict(req, &rows)?;
            let scored = score_candidates(req, rows, preds, result.history.len() + 1)?;
            advance(&mut result, scored, t, &mut parents);
            if let Some(generation) = result.history.last() {
                progress(generation);
            }
        }
    }
    Ok(result)
}
fn advance(
    result: &mut WorkResult,
    scored: Vec<Candidate>,
    t: &TournamentConfig,
    parents: &mut Vec<Vec<String>>,
) {
    let total = scored.len();
    let mut ranked: Vec<_> = scored.into_iter().filter(|s| s.reaped.is_none()).collect();
    ranked.sort_by(|a, b| {
        b.hit
            .partial_cmp(&a.hit)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let keep = ((ranked.len() as f64 * (1. - t.reap)).ceil() as usize)
        .max(1)
        .min(ranked.len());
    ranked.truncate(keep);
    result.judged += total;
    result.reaped += total - keep;
    let mut seen: BTreeSet<_> = result.archive.iter().map(|c| c.row.clone()).collect();
    for c in &ranked {
        if seen.insert(c.row.clone()) {
            result.archive.push(c.clone());
        }
    }
    result.archive.sort_by(|a, b| {
        b.hit
            .partial_cmp(&a.hit)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    result.archive.truncate(60);
    let mut unique = BTreeSet::new();
    *parents = ranked
        .iter()
        .filter(|c| unique.insert(c.row.clone()))
        .take((t.population as f64 * t.elite).round().max(2.) as usize)
        .map(|c| c.row.clone())
        .collect();
    result.history.push(Generation {
        generation: result.history.len() + 1,
        best: result.archive.first().and_then(|c| c.hit),
        median: ranked.get(ranked.len() / 2).and_then(|c| c.hit),
        alive: keep,
        reaped: total - keep,
        judged: result.judged,
        champions: ranked.iter().take(6).cloned().collect(),
    });
}
