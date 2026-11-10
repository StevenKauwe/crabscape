//! Actual TabICL models, fitted independently for each outcome. No synthetic scores.
use crate::{Dataset, Kind, Prediction, Role, WorkRequest};
use std::{collections::BTreeMap, sync::Arc};
use tabicl_anywhere::{
    csv::Schema,
    fit::{FitOpts, Kv},
    model::{Load, Model},
    predict::{IclWeights, PredictOpts, Stats},
    store::{Bundle, MemStore},
    table::{Classifier, ClassifierOpts, Regressor, fit_classifier_on, fit_regressor_on},
};
struct Fitted {
    name: String,
    kind: Kind,
    schema: Schema,
    bytes: Arc<Vec<u8>>,
}
pub struct Oracle {
    reg: Option<Model>,
    class: Option<Model>,
    reg_bytes: Option<Vec<u8>>,
    class_bytes: Option<Vec<u8>>,
    fitted: Vec<Fitted>,
}
fn error(e: impl ToString) -> String {
    e.to_string()
}
fn model(bytes: Vec<u8>) -> Result<Model, String> {
    let bytes = tabicl_anywhere::ckpt::bundle_bytes(bytes, false).map_err(error)?;
    Model::load(
        &Bundle::from_source(Box::new(Arc::new(bytes))).map_err(error)?,
        Load::Fit,
    )
    .map_err(error)
}
impl Oracle {
    pub fn new(reg: Option<Vec<u8>>, class: Option<Vec<u8>>) -> Self {
        Self {
            reg: None,
            class: None,
            reg_bytes: reg,
            class_bytes: class,
            fitted: vec![],
        }
    }
    pub fn fit(&mut self, req: &WorkRequest) -> Result<(), String> {
        self.fit_cancellable(req, || false)
    }
    pub fn fit_cancellable(
        &mut self,
        req: &WorkRequest,
        cancelled: impl Fn() -> bool,
    ) -> Result<(), String> {
        let (names, features) = feature_rows(req, &req.dataset.rows, false)?;
        for (idx, c) in req
            .columns
            .iter()
            .enumerate()
            .filter(|(_, c)| c.role == Role::Output)
        {
            if cancelled() {
                return Err("Tournament cancelled".into());
            }
            let mut table = vec![
                names
                    .iter()
                    .cloned()
                    .chain(std::iter::once(c.name.clone()))
                    .collect::<Vec<_>>(),
            ];
            for (r, x) in req.dataset.rows.iter().zip(&features) {
                if let Some(y) = r.get(idx).filter(|v| !v.trim().is_empty()) {
                    let mut row = x.clone();
                    row.push(y.clone());
                    table.push(row);
                }
            }
            if table.len() < 11 {
                return Err(format!("{} needs at least 10 labelled rows", c.name));
            }
            let store = MemStore::new();
            let opts = ClassifierOpts {
                n_estimators: 1,
                fit: FitOpts {
                    kv: Kv::I16,
                    ..Default::default()
                },
                ..Default::default()
            };
            let schema = match c.kind {
                Kind::Number => {
                    if self.reg.is_none() {
                        self.reg = Some(model(
                            self.reg_bytes.take().ok_or("Regressor weights missing")?,
                        )?);
                    }
                    let (schema, x, y) = Schema::learn_numeric(&table, &c.name).map_err(error)?;
                    fit_regressor_on(
                        self.reg.as_ref().ok_or("Regressor missing")?,
                        &x,
                        &y,
                        y.len(),
                        schema.columns.len(),
                        Box::new(store.clone()),
                        &opts,
                        &[],
                    )
                    .map_err(error)?;
                    schema
                }
                Kind::Category => {
                    if self.class.is_none() {
                        self.class = Some(model(
                            self.class_bytes
                                .take()
                                .ok_or("Classifier weights missing")?,
                        )?);
                    }
                    let (schema, x, y) = Schema::learn(&table, &c.name).map_err(error)?;
                    if !(2..=20).contains(&schema.labels.len()) {
                        return Err("Classification needs 2-20 classes".into());
                    }
                    fit_classifier_on(
                        self.class.as_ref().ok_or("Classifier missing")?,
                        &x,
                        &y,
                        y.len(),
                        schema.columns.len(),
                        Box::new(store.clone()),
                        &opts,
                        &[],
                    )
                    .map_err(error)?;
                    schema
                }
            };
            self.fitted.push(Fitted {
                name: c.name.clone(),
                kind: c.kind.clone(),
                schema,
                bytes: Arc::new(store.bytes()),
            });
        }
        Ok(())
    }
    pub fn predict(
        &self,
        req: &WorkRequest,
        rows: &[Vec<String>],
    ) -> Result<BTreeMap<String, Vec<Prediction>>, String> {
        let (names, values) = feature_rows(req, rows, true)?;
        let table: Vec<_> = std::iter::once(names).chain(values).collect();
        let mut out = BTreeMap::new();
        for f in &self.fitted {
            let x = f.schema.features(&table).map_err(error)?;
            let b = Bundle::from_source(Box::new(f.bytes.clone())).map_err(error)?;
            let opts = PredictOpts {
                rows_per_pass: 256,
                ..Default::default()
            };
            let predictions = match f.kind {
                Kind::Number => {
                    let r = Regressor::open(&b)
                        .map_err(error)?
                        .predict(
                            self.reg.as_ref().ok_or("Regressor missing")?,
                            &mut IclWeights::Resident,
                            &x,
                            rows.len(),
                            &[0.05, 0.95],
                            &opts,
                            &Stats::default(),
                        )
                        .map_err(error)?;
                    (0..rows.len())
                        .map(|i| Prediction::Numeric {
                            mean: r.mean[i],
                            median: r.median[i],
                            q05: r.quantiles[2 * i],
                            q95: r.quantiles[2 * i + 1],
                        })
                        .collect()
                }
                Kind::Category => {
                    let p = Classifier::open(&b)
                        .map_err(error)?
                        .predict_proba(
                            self.class.as_ref().ok_or("Classifier missing")?,
                            &mut IclWeights::Resident,
                            &x,
                            rows.len(),
                            &opts,
                            &Stats::default(),
                        )
                        .map_err(error)?;
                    let k = f.schema.labels.len();
                    p.chunks_exact(k)
                        .map(|p| Prediction::Categorical {
                            labels: f.schema.labels.clone(),
                            proba: p.to_vec(),
                        })
                        .collect()
                }
            };
            out.insert(f.name.clone(), predictions);
        }
        Ok(out)
    }
}
fn feature_rows(
    req: &WorkRequest,
    rows: &[Vec<String>],
    candidate: bool,
) -> Result<(Vec<String>, Vec<Vec<String>>), String> {
    let inputs: Vec<_> = req
        .columns
        .iter()
        .enumerate()
        .filter(|(_, c)| c.role == Role::Input)
        .collect();
    if inputs.is_empty() {
        return Err("Choose at least one input".into());
    }
    let mut names = vec![];
    for (_, c) in &inputs {
        if let Some(l) = &c.lookup {
            for &field in &l.fields {
                let name = l.table.headers.get(field).ok_or("Lookup field missing")?;
                names.push(format!("{}.{}", l.alias, name));
            }
        } else {
            names.push(c.name.clone());
        }
    }
    if names.is_empty() {
        return Err("Choose at least one feature".into());
    }
    let values = rows
        .iter()
        .map(|r| {
            let mut out = vec![];
            for (j, (idx, c)) in inputs.iter().enumerate() {
                let v = r
                    .get(if candidate { j } else { *idx })
                    .cloned()
                    .unwrap_or_default();
                if let Some(l) = &c.lookup {
                    if v.is_empty() {
                        out.extend(l.fields.iter().map(|_| String::new()));
                        continue;
                    }
                    let found = l
                        .table
                        .rows
                        .iter()
                        .find(|row| row.get(l.key) == Some(&v))
                        .ok_or_else(|| format!("No lookup key {v} for {}", c.name))?;
                    for &field in &l.fields {
                        out.push(found.get(field).cloned().unwrap_or_default());
                    }
                } else {
                    out.push(v);
                }
            }
            Ok(out)
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok((names, values))
}
#[allow(dead_code)]
fn _dataset_contract(_: Dataset) {}
