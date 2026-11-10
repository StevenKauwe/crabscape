use gridcraft_core::{CellRef, Value};
use gridcraft_model::{Cell, Workbook};
use serde::{Deserialize, Serialize};
fn err(e: impl ToString) -> String {
    e.to_string()
}
pub struct Book {
    wb: Workbook,
    warnings: Vec<String>,
}
impl Book {
    pub fn new(bytes: &[u8], xlsx: bool) -> Result<Book, String> {
        if bytes.len() > 20_000_000 {
            return Err(err("Spike limit: 20 MB per file"));
        }
        let (wb, warnings) = if xlsx {
            let (w, r) = gridcraft_xlsx::read_xlsx(bytes).map_err(err)?;
            (w, r.warnings)
        } else {
            (
                gridcraft_xlsx::read_csv(
                    bytes,
                    &gridcraft_xlsx::CsvOptions {
                        delimiter: 0,
                        ..Default::default()
                    },
                )
                .map_err(err)?,
                vec![],
            )
        };
        Ok(Book { wb, warnings })
    }
    pub fn snapshot(&self, sheet: usize) -> Result<String, String> {
        let s = self.wb.sheet(sheet).ok_or_else(|| err("Sheet missing"))?;
        let mut rows = 0usize;
        let mut cols = 0usize;
        let mut formulas = 0usize;
        for (p, c) in s.cells.iter() {
            rows = rows.max(p.row as usize + 1);
            cols = cols.max(p.col as usize + 1);
            if c.formula.is_some() {
                formulas += 1;
            }
        }
        if rows > 5001 || cols > 64 {
            return Err(err(
                "Spike limit: 5,000 data rows and 64 columns. Prepare a smaller table.",
            ));
        }
        let mut data = vec![vec![String::new(); cols]; rows];
        for (p, c) in s.cells.iter() {
            if let Some(v) = data
                .get_mut(p.row as usize)
                .and_then(|r| r.get_mut(p.col as usize))
            {
                *v = c.value.display();
            }
        }
        serde_json::to_string(&serde_json::json!({"sheets":self.wb.sheets.iter().map(|s|s.name.clone()).collect::<Vec<_>>(),"data":data,"warnings":self.warnings,"formulas":formulas})).map_err(err)
    }
    pub fn set(
        &mut self,
        sheet: usize,
        row: u32,
        col: u32,
        value: &str,
        numeric: bool,
    ) -> Result<(), String> {
        if row > 5000 || col > 63 {
            return Err(err("Cell outside spike limits"));
        }
        let v = if value.is_empty() {
            Value::Empty
        } else if numeric {
            let n = value.parse::<f64>().map_err(err)?;
            if !n.is_finite() {
                return Err(err("Use a finite number"));
            }
            Value::Number(n)
        } else {
            Value::Text(value.into())
        };
        self.wb
            .sheet_mut(sheet)
            .ok_or_else(|| err("Sheet missing"))?
            .cells
            .set(CellRef { row, col }, Cell::value(v));
        Ok(())
    }
    pub fn export(&self) -> Result<Vec<u8>, String> {
        gridcraft_xlsx::write_xlsx(&self.wb).map_err(err)
    }
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Domain {
    pub ranges: Option<Vec<[f64; 2]>>,
    pub classes: Option<Vec<String>>,
    #[serde(default = "required")]
    pub presence: String,
}
fn required() -> String {
    "required".into()
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Collection {
    pub name: String,
    pub columns: Vec<usize>,
    pub counts: Vec<usize>,
    pub sum: Option<[f64; 2]>,
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Request {
    pub domains: Vec<Domain>,
    #[serde(default)]
    pub collections: Vec<Collection>,
}
#[derive(Clone, Debug)]
enum Choice {
    Empty,
    Numeric(f64, f64),
    Category,
}
impl Choice {
    fn present(&self) -> bool {
        !matches!(self, Self::Empty)
    }
    fn bounds(&self) -> (f64, f64) {
        match self {
            Self::Numeric(a, b) => (*a, *b),
            _ => (0., 0.),
        }
    }
}
struct Rng(u32);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        self.0 as f64 / 4294967296.
    }
    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = (self.next() * (i + 1) as f64) as usize;
            v.swap(i, j);
        }
    }
}
fn options(d: &Domain) -> Result<Vec<Choice>, String> {
    if !["required", "optional", "empty"].contains(&d.presence.as_str()) {
        return Err("Unknown presence rule".into());
    }
    let mut out = vec![];
    if d.presence != "required" {
        out.push(Choice::Empty);
    }
    if d.presence != "empty" {
        match (&d.ranges, &d.classes) {
            (Some(r), None) => {
                if r.len() > 16
                    || r.iter().any(|[a, b]| {
                        !a.is_finite()
                            || !b.is_finite()
                            || a > b
                            || a.abs() > 1e12
                            || b.abs() > 1e12
                    })
                {
                    return Err(
                        "Ranges must be finite, ordered, within ±1e12 (max 16 ranges)".into(),
                    );
                }
                out.extend(r.iter().map(|[a, b]| Choice::Numeric(*a, *b)));
            }
            (None, Some(c)) => {
                if c.iter().any(|s| s.is_empty()) {
                    return Err("Use presence rules for empty categories".into());
                }
                if !c.is_empty() {
                    out.push(Choice::Category);
                }
            }
            _ => return Err("Choose numeric ranges or categorical classes".into()),
        }
    }
    if out.is_empty() {
        return Err("A required column has no allowed values".into());
    }
    Ok(out)
}
fn validate(req: &Request) -> Result<Vec<Vec<Choice>>, String> {
    if req.domains.is_empty() || req.domains.len() > 64 || req.collections.len() > 12 {
        return Err("Limit: 1–64 inputs and 12 collections".into());
    }
    let opts = req
        .domains
        .iter()
        .map(options)
        .collect::<Result<Vec<_>, _>>()?;
    for g in &req.collections {
        let unique: std::collections::BTreeSet<_> = g.columns.iter().copied().collect();
        if g.columns.is_empty()
            || unique.len() != g.columns.len()
            || g.columns.iter().any(|&i| i >= opts.len())
            || g.counts.is_empty()
            || g.counts.iter().any(|&c| c > g.columns.len())
        {
            return Err(format!(
                "{}: choose distinct columns and valid allowed counts",
                g.name
            ));
        }
        if let Some([lo, hi]) = g.sum {
            if !lo.is_finite() || !hi.is_finite() || lo > hi || lo.abs() > 1e12 || hi.abs() > 1e12 {
                return Err(format!("{}: invalid sum range", g.name));
            }
            if g.columns.iter().any(|&i| req.domains[i].ranges.is_none()) {
                return Err(format!("{}: sums require numeric columns", g.name));
            }
        }
    }
    Ok(opts)
}
fn possible(chosen: &[Choice], opts: &[Vec<Choice>], groups: &[Collection]) -> bool {
    for g in groups {
        let (mut min_count, mut max_count) = (0, 0);
        let (mut lo, mut hi) = (0., 0.);
        for &i in &g.columns {
            if let Some(c) = chosen.get(i) {
                min_count += usize::from(c.present());
                max_count += usize::from(c.present());
                let (a, b) = c.bounds();
                lo += a;
                hi += b;
            } else {
                min_count += usize::from(opts[i].iter().all(Choice::present));
                max_count += usize::from(opts[i].iter().any(Choice::present));
                lo += opts[i]
                    .iter()
                    .map(|c| c.bounds().0)
                    .fold(f64::INFINITY, f64::min);
                hi += opts[i]
                    .iter()
                    .map(|c| c.bounds().1)
                    .fold(f64::NEG_INFINITY, f64::max);
            }
        }
        if !g.counts.iter().any(|&n| n >= min_count && n <= max_count) {
            return false;
        }
        if let Some([a, b]) = g.sum
            && (hi < a || lo > b)
        {
            return false;
        }
    }
    true
}
fn solve(chosen: &[Choice], groups: &[Collection], rng: &mut Rng) -> Option<Vec<f64>> {
    use minilp::{ComparisonOp, OptimizationDirection, Problem};
    let mut p = Problem::new(OptimizationDirection::Maximize);
    let vars: Vec<_> = chosen
        .iter()
        .map(|c| p.add_var(rng.next() * 2. - 1., c.bounds()))
        .collect();
    for g in groups {
        if let Some([lo, hi]) = g.sum {
            let terms: Vec<_> = g.columns.iter().map(|&i| (vars[i], 1.)).collect();
            p.add_constraint(&terms, ComparisonOp::Ge, lo);
            p.add_constraint(&terms, ComparisonOp::Le, hi);
        }
    }
    p.solve().ok().map(|s| vars.iter().map(|&v| s[v]).collect())
}
fn search(
    chosen: &mut Vec<Choice>,
    opts: &[Vec<Choice>],
    groups: &[Collection],
    rng: &mut Rng,
    budget: &mut usize,
) -> Option<(Vec<Choice>, Vec<f64>)> {
    if *budget == 0 {
        return None;
    }
    *budget -= 1;
    if !possible(chosen, opts, groups) {
        return None;
    }
    if chosen.len() == opts.len() {
        return solve(chosen, groups, rng).map(|v| (chosen.clone(), v));
    }
    let mut order = opts[chosen.len()].clone();
    rng.shuffle(&mut order);
    for c in order {
        chosen.push(c);
        let found = search(chosen, opts, groups, rng, budget);
        chosen.pop();
        if found.is_some() {
            return found;
        }
        if *budget == 0 {
            break;
        }
    }
    None
}
pub fn valid_row(row: &[String], req: &Request) -> bool {
    if row.len() != req.domains.len() {
        return false;
    }
    for (v, d) in row.iter().zip(&req.domains) {
        if v.is_empty() {
            if d.presence == "required" {
                return false;
            }
        } else {
            if d.presence == "empty" {
                return false;
            }
            if let Some(r) = &d.ranges {
                let Ok(x) = v.parse::<f64>() else {
                    return false;
                };
                if !x.is_finite() || !r.iter().any(|[a, b]| x >= *a && x <= *b) {
                    return false;
                }
            } else if !d.classes.as_ref().is_some_and(|c| c.contains(v)) {
                return false;
            }
        }
    }
    for g in &req.collections {
        let count = g.columns.iter().filter(|&&i| !row[i].is_empty()).count();
        if !g.counts.contains(&count) {
            return false;
        }
        if let Some([a, b]) = g.sum {
            let sum: f64 = g
                .columns
                .iter()
                .filter(|&&i| !row[i].is_empty())
                .map(|&i| row[i].parse::<f64>().unwrap_or(f64::NAN))
                .sum();
            if !sum.is_finite() || sum < a || sum > b {
                return false;
            }
        }
    }
    true
}
#[derive(Debug)]
struct Partition {
    groups: Vec<usize>,
    required: Vec<usize>,
    optional: Vec<usize>,
}
#[derive(Debug)]
struct SupportSet {
    supports: Vec<Vec<bool>>,
    complete: bool,
    nodes: usize,
    partitions: usize,
    overlapping: bool,
}
fn membership_partitions(opts: &[Vec<Choice>], groups: &[Collection]) -> Vec<Partition> {
    let mut map = std::collections::BTreeMap::<Vec<usize>, Partition>::new();
    for (i, choices) in opts.iter().enumerate() {
        let membership: Vec<_> = groups
            .iter()
            .enumerate()
            .filter_map(|(g, c)| c.columns.contains(&i).then_some(g))
            .collect();
        let p = map.entry(membership.clone()).or_insert(Partition {
            groups: membership,
            required: vec![],
            optional: vec![],
        });
        if choices.iter().all(Choice::present) {
            p.required.push(i);
        } else if choices.iter().any(Choice::present) {
            p.optional.push(i);
        }
    }
    map.into_values().collect()
}
// First solve integer counts for each membership-equivalent partition. Then expand
// actual column identities: identical membership does NOT imply identical domains.
fn enumerate_supports(
    opts: &[Vec<Choice>],
    groups: &[Collection],
    max_nodes: usize,
    max_supports: usize,
) -> SupportSet {
    let parts = membership_partitions(opts, groups);
    let mut out = SupportSet {
        supports: vec![],
        complete: true,
        nodes: 0,
        partitions: parts.len(),
        overlapping: parts.iter().any(|p| p.groups.len() > 1),
    };
    fn tick(out: &mut SupportSet, max: usize) -> bool {
        if out.nodes >= max {
            out.complete = false;
            false
        } else {
            out.nodes += 1;
            true
        }
    }
    fn identities(
        p: usize,
        parts: &[Partition],
        counts: &[usize],
        support: &mut Vec<bool>,
        out: &mut SupportSet,
        limit: usize,
        cap: usize,
    ) {
        if !out.complete || !tick(out, limit) {
            return;
        }
        if p == parts.len() {
            if out.supports.len() >= cap {
                out.complete = false;
            } else {
                out.supports.push(support.clone());
            }
            return;
        }
        for &i in &parts[p].required {
            support[i] = true;
        }
        #[allow(clippy::too_many_arguments)] // Bounded recursive enumeration context.
        fn combinations(
            pos: usize,
            left: usize,
            p: usize,
            parts: &[Partition],
            counts: &[usize],
            s: &mut Vec<bool>,
            out: &mut SupportSet,
            limit: usize,
            cap: usize,
        ) {
            if !out.complete || !tick(out, limit) {
                return;
            }
            let vars = &parts[p].optional;
            if left > vars.len() - pos {
                return;
            }
            if pos == vars.len() {
                if left == 0 {
                    identities(p + 1, parts, counts, s, out, limit, cap);
                }
                return;
            }
            let i = vars[pos];
            s[i] = false;
            combinations(pos + 1, left, p, parts, counts, s, out, limit, cap);
            if left > 0 {
                s[i] = true;
                combinations(pos + 1, left - 1, p, parts, counts, s, out, limit, cap);
                s[i] = false;
            }
        }
        combinations(0, counts[p], p, parts, counts, support, out, limit, cap);
    }
    #[allow(clippy::too_many_arguments)] // Bounded recursive enumeration context.
    fn counts(
        p: usize,
        parts: &[Partition],
        groups: &[Collection],
        totals: &mut Vec<usize>,
        chosen: &mut Vec<usize>,
        width: usize,
        out: &mut SupportSet,
        limit: usize,
        cap: usize,
    ) {
        if !out.complete || !tick(out, limit) {
            return;
        }
        // Exact integer lower/upper bounds prune impossible count vectors.
        for (g, rule) in groups.iter().enumerate() {
            let min = totals[g]
                + parts[p..]
                    .iter()
                    .filter(|x| x.groups.contains(&g))
                    .map(|x| x.required.len())
                    .sum::<usize>();
            let max = min
                + parts[p..]
                    .iter()
                    .filter(|x| x.groups.contains(&g))
                    .map(|x| x.optional.len())
                    .sum::<usize>();
            if !rule.counts.iter().any(|&c| c >= min && c <= max) {
                return;
            }
        }
        if p == parts.len() {
            identities(0, parts, chosen, &mut vec![false; width], out, limit, cap);
            return;
        }
        // With disjoint groups this is direct: only the local group's allowed count
        // choices survive, with no coupled count constraints to solve.
        for n in 0..=parts[p].optional.len() {
            for &g in &parts[p].groups {
                totals[g] += parts[p].required.len() + n;
            }
            chosen.push(n);
            counts(p + 1, parts, groups, totals, chosen, width, out, limit, cap);
            chosen.pop();
            for &g in &parts[p].groups {
                totals[g] -= parts[p].required.len() + n;
            }
            if !out.complete {
                break;
            }
        }
    }
    counts(
        0,
        &parts,
        groups,
        &mut vec![0; groups.len()],
        &mut vec![],
        opts.len(),
        &mut out,
        max_nodes,
        max_supports,
    );
    out
}
#[derive(Serialize, Debug)]
pub struct Diagnostics {
    partitions: usize,
    method: String,
    support_enumeration_complete: bool,
    count_valid_supports_enumerated: usize,
    supports_with_numeric_witness: usize,
    numeric_search_complete: bool,
    numeric_rejected_or_unresolved: usize,
    support_nodes: usize,
    numeric_nodes: usize,
    distinct_supports_sampled: usize,
}
#[derive(Serialize)]
pub struct SampleResult {
    pub rows: Vec<Vec<String>>,
    pub diagnostics: Diagnostics,
}
struct Witness {
    support: Vec<bool>,
    choices: Vec<Choice>,
    values: Vec<f64>,
}
fn row_from(
    chosen: &[Choice],
    a: &[f64],
    b: &[f64],
    req: &Request,
    rng: &mut Rng,
) -> Result<Vec<String>, String> {
    let mix = rng.next();
    let mut row = vec![];
    for (i, c) in chosen.iter().enumerate() {
        row.push(match c {
            Choice::Empty => String::new(),
            Choice::Category => {
                let classes = req.domains[i].classes.as_ref().ok_or("Missing classes")?;
                classes[(rng.next() * classes.len() as f64) as usize].clone()
            }
            Choice::Numeric(lo, hi) => {
                let v = if req
                    .collections
                    .iter()
                    .any(|g| g.sum.is_some() && g.columns.contains(&i))
                {
                    (a[i] * (1. - mix) + b[i] * mix).clamp(*lo, *hi)
                } else {
                    let u = rng.next();
                    lo * (1. - u) + hi * u
                };
                v.to_string()
            }
        });
    }
    Ok(row)
}
fn catalog(
    req: &Request,
    opts: &[Vec<Choice>],
    rng: &mut Rng,
) -> Result<(Vec<Witness>, Diagnostics), String> {
    let set = enumerate_supports(opts, &req.collections, 200_000, 4096);
    if set.supports.is_empty() {
        return Err(if set.complete{"Infeasible: exhaustive integer support enumeration proves no support satisfies all count and presence rules."}else{"Search budget exhausted before finding a count-valid support. Feasibility is unresolved, not proven impossible."}.into());
    }
    let mut diag = Diagnostics {
        partitions: set.partitions,
        method: if set.overlapping {
            "membership-partition enumeration"
        } else {
            "direct disjoint-group count combinations"
        }
        .into(),
        support_enumeration_complete: set.complete,
        count_valid_supports_enumerated: set.supports.len(),
        supports_with_numeric_witness: 0,
        numeric_search_complete: true,
        numeric_rejected_or_unresolved: 0,
        support_nodes: set.nodes,
        numeric_nodes: 0,
        distinct_supports_sampled: 0,
    };
    let mut witnesses = vec![];
    let mut remaining = 200_000usize;
    for support in set.supports {
        if remaining == 0 {
            diag.numeric_search_complete = false;
            diag.numeric_rejected_or_unresolved =
                diag.count_valid_supports_enumerated - witnesses.len();
            break;
        }
        let restricted: Vec<Vec<Choice>> = opts
            .iter()
            .enumerate()
            .map(|(i, options)| {
                options
                    .iter()
                    .filter(|c| c.present() == support[i])
                    .cloned()
                    .collect()
            })
            .collect();
        let mut budget = remaining.min(2048);
        let start = budget;
        let result = search(&mut vec![], &restricted, &req.collections, rng, &mut budget);
        remaining -= start - budget;
        diag.numeric_nodes += start - budget;
        if let Some((choices, values)) = result {
            // A floating-point LP answer is only a witness after strict row validation.
            let mut row = row_from(&choices, &values, &values, req, rng)?;
            for (i, c) in choices.iter().enumerate() {
                if matches!(c, Choice::Numeric(_, _)) {
                    row[i] = values[i].to_string();
                }
            }
            if valid_row(&row, req) {
                witnesses.push(Witness {
                    support,
                    choices,
                    values,
                });
            } else {
                diag.numeric_rejected_or_unresolved += 1;
                diag.numeric_search_complete = false;
            }
        } else {
            diag.numeric_rejected_or_unresolved += 1;
            if budget == 0 {
                diag.numeric_search_complete = false;
            }
        }
    }
    diag.supports_with_numeric_witness = witnesses.len();
    if witnesses.is_empty() {
        return Err(if !diag.support_enumeration_complete||!diag.numeric_search_complete{"Search budget/precision limit: no verified numeric witness found. Feasibility is unresolved; no candidates returned."}else{"No numeric witness: all enumerated supports were rejected by interval bounds or the floating-point LP solver. This is not an exact-arithmetic proof of numeric infeasibility. No candidates returned."}.into());
    }
    Ok((witnesses, diag))
}
pub fn sample_result(req: &Request, count: usize, seed: u32) -> Result<SampleResult, String> {
    if count == 0 || count > 512 {
        return Err("Choose 1–512 candidates".into());
    }
    let opts = validate(req)?;
    let mut rng = Rng(seed);
    let (witnesses, mut diagnostics) = catalog(req, &opts, &mut rng)?;
    let mut order: Vec<_> = (0..witnesses.len()).collect();
    rng.shuffle(&mut order);
    let mut rows = vec![];
    let mut represented = std::collections::BTreeSet::new();
    let mut attempts = 0;
    while rows.len() < count && attempts < count * 8 + 64 {
        let w = &witnesses[order[attempts % order.len()]];
        attempts += 1;
        // Revisit interval choices within this fixed support; the cached strict witness
        // is a safe fallback. Interval-region coverage is never claimed exhaustive.
        let restricted: Vec<Vec<Choice>> = opts
            .iter()
            .enumerate()
            .map(|(i, v)| {
                v.iter()
                    .filter(|c| c.present() == w.support[i])
                    .cloned()
                    .collect()
            })
            .collect();
        let mut budget = (200_000usize.saturating_sub(diagnostics.numeric_nodes)).min(2048);
        let start = budget;
        let variant = if budget > 0 {
            search(
                &mut vec![],
                &restricted,
                &req.collections,
                &mut rng,
                &mut budget,
            )
        } else {
            None
        };
        diagnostics.numeric_nodes += start - budget;
        let (choices, a) = variant
            .as_ref()
            .map(|(c, v)| (c.as_slice(), v.as_slice()))
            .unwrap_or((&w.choices, &w.values));
        let b = solve(choices, &req.collections, &mut rng).unwrap_or_else(|| a.to_vec());
        let mut row = row_from(choices, a, &b, req, &mut rng)?;
        if !valid_row(&row, req) {
            row = row_from(&w.choices, &w.values, &w.values, req, &mut rng)?;
            for (i, c) in w.choices.iter().enumerate() {
                if matches!(c, Choice::Numeric(_, _)) {
                    row[i] = w.values[i].to_string();
                }
            }
        }
        if valid_row(&row, req) {
            represented.insert(w.support.clone());
            rows.push(row);
        }
    }
    if rows.len() != count {
        return Err(
            "Numeric precision budget exhausted; no unverified candidate batch returned.".into(),
        );
    }
    diagnostics.distinct_supports_sampled = represented.len();
    Ok(SampleResult { rows, diagnostics })
}
#[cfg(test)]
fn sample_impl(req: &Request, count: usize, seed: u32) -> Result<Vec<Vec<String>>, String> {
    sample_result(req, count, seed).map(|r| r.rows)
}
pub fn sample(request: &str, count: usize, seed: u32) -> Result<String, String> {
    let req: Request = serde_json::from_str(request).map_err(err)?;
    serde_json::to_string(&sample_result(&req, count, seed).map_err(err)?).map_err(err)
}
#[derive(Serialize)]
pub struct EvolutionResult {
    pub rows: Vec<Vec<String>>,
    pub diagnostics: Diagnostics,
    pub rejected_parents: usize,
    pub crossover_children: usize,
    pub explorers: usize,
    pub mutation_fallbacks: usize,
}
pub fn evolve_result(
    req: &Request,
    parents: &[Vec<String>],
    count: usize,
    seed: u32,
) -> Result<EvolutionResult, String> {
    let fresh = sample_result(req, count, seed)?;
    let valid: Vec<_> = parents.iter().filter(|p| valid_row(p, req)).collect();
    let rejected_parents = parents.len() - valid.len();
    let mut rows = vec![];
    let mut rng = Rng(seed.wrapping_add(0x9e3779b9));
    let (mut crossover_children, mut explorers, mut mutation_fallbacks) = (0, 0, 0);
    for (i, explorer) in fresh.rows.iter().enumerate() {
        // 35% fresh support/interval immigrants ensure exploration outside parent supports.
        if valid.is_empty() || rng.next() < 0.35 {
            rows.push(explorer.clone());
            explorers += 1;
            continue;
        }
        let a = valid[(rng.next() * valid.len() as f64) as usize];
        let compatible: Vec<_> = valid
            .iter()
            .filter(|b| {
                a.iter().zip(b.iter()).enumerate().all(|(j, (x, y))| {
                    if x.is_empty() || y.is_empty() {
                        return x.is_empty() && y.is_empty();
                    }
                    if let Some(ranges) = &req.domains[j].ranges {
                        let (Ok(x), Ok(y)) = (x.parse::<f64>(), y.parse::<f64>()) else {
                            return false;
                        };
                        ranges
                            .iter()
                            .any(|[lo, hi]| x >= *lo && x <= *hi && y >= *lo && y <= *hi)
                    } else {
                        true
                    }
                })
            })
            .collect();
        let b = compatible[(rng.next() * compatible.len() as f64) as usize];
        let mut child = vec![];
        let mix = rng.next();
        let mut box_choices = vec![];
        for (j, (x, y)) in a.iter().zip(b.iter()).enumerate() {
            if x.is_empty() {
                child.push(String::new());
                box_choices.push(Choice::Empty);
            } else if let Some(ranges) = &req.domains[j].ranges {
                let ax = x.parse::<f64>().map_err(|_| "Invalid numeric parent")?;
                let by = y.parse::<f64>().map_err(|_| "Invalid numeric parent")?;
                let bounds = ranges
                    .iter()
                    .find(|[lo, hi]| ax >= *lo && ax <= *hi && by >= *lo && by <= *hi)
                    .ok_or("No compatible parental interval")?;
                child.push(
                    (ax * (1. - mix) + by * mix)
                        .clamp(bounds[0], bounds[1])
                        .to_string(),
                );
                box_choices.push(Choice::Numeric(bounds[0], bounds[1]));
            } else {
                let classes = req.domains[j]
                    .classes
                    .as_ref()
                    .ok_or("Missing categories")?;
                child.push(if rng.next() < 0.25 {
                    classes[(rng.next() * classes.len() as f64) as usize].clone()
                } else if rng.next() < 0.5 {
                    x.clone()
                } else {
                    y.clone()
                });
                box_choices.push(Choice::Category);
            }
        }
        // A shared convex weight preserves ALL overlapping linear sums. Mutation moves
        // toward a feasible LP vertex in the same support/interval box; no coordinate
        // mutation is allowed to break a sum constraint.
        if let Some(vertex) = solve(&box_choices, &req.collections, &mut rng) {
            let amount = 0.05 + rng.next() * 0.3;
            for (j, c) in box_choices.iter().enumerate() {
                if let Choice::Numeric(lo, hi) = c {
                    let value = child[j].parse::<f64>().map_err(|_| "Invalid child")?;
                    child[j] = (value * (1. - amount) + vertex[j] * amount)
                        .clamp(*lo, *hi)
                        .to_string();
                }
            }
        }
        if valid_row(&child, req) {
            rows.push(child);
            crossover_children += 1;
        } else {
            rows.push(fresh.rows[i].clone());
            mutation_fallbacks += 1;
        }
    }
    if !rows.iter().all(|r| valid_row(r, req)) {
        return Err("Evolution validation failed; no population returned".into());
    }
    Ok(EvolutionResult {
        rows,
        diagnostics: fresh.diagnostics,
        rejected_parents,
        crossover_children,
        explorers,
        mutation_fallbacks,
    })
}
pub fn evolve(request: &str, parents: &str, count: usize, seed: u32) -> Result<String, String> {
    let req: Request = serde_json::from_str(request).map_err(err)?;
    let parents: Vec<Vec<String>> = serde_json::from_str(parents).map_err(err)?;
    if parents.len() > 512 {
        return Err(err("At most 512 parents"));
    }
    serde_json::to_string(&evolve_result(&req, &parents, count, seed).map_err(err)?).map_err(err)
}
pub fn eval_node(
    n: &serde_json::Value,
    values: &serde_json::Value,
    depth: usize,
    nodes: &mut usize,
) -> Result<Option<f64>, String> {
    *nodes += 1;
    if depth > 32 || *nodes > 200 {
        return Err("Formula complexity limit".into());
    }
    let op = n
        .get("op")
        .and_then(|v| v.as_str())
        .ok_or("Missing formula operator")?;
    let child = |key: &str| n.get(key).ok_or_else(|| format!("Missing {key}"));
    let result = match op {
        "number" => Some(child("v")?.as_f64().ok_or("Invalid constant")?),
        "ref" => {
            let name = child("name")?.as_str().ok_or("Invalid reference")?;
            match values.get(name) {
                None | Some(serde_json::Value::Null) => None,
                Some(serde_json::Value::String(s)) if s.is_empty() => None,
                Some(serde_json::Value::String(s)) => Some(
                    s.trim()
                        .parse::<f64>()
                        .map_err(|_| "Non-numeric formula input")?,
                ),
                Some(v) => Some(v.as_f64().ok_or("Non-numeric formula input")?),
            }
        }
        "unary" => eval_node(child("value")?, values, depth + 1, nodes)?.map(|v| {
            if n.get("sign").and_then(|s| s.as_str()) == Some("-") {
                -v
            } else {
                v
            }
        }),
        "call" => {
            let name = child("name")?.as_str().ok_or("Invalid function")?;
            let args = child("args")?
                .as_array()
                .ok_or("Missing function arguments")?;
            let a = eval_node(
                args.first().ok_or("Missing argument")?,
                values,
                depth + 1,
                nodes,
            )?;
            match name {
                "abs" if args.len() == 1 => a.map(f64::abs),
                "coalesce" if args.len() == 2 => {
                    if a.is_some() {
                        a
                    } else {
                        eval_node(&args[1], values, depth + 1, nodes)?
                    }
                }
                "min" | "max" if args.len() == 2 => {
                    let b = eval_node(&args[1], values, depth + 1, nodes)?;
                    a.zip(b)
                        .map(|(a, b)| if name == "min" { a.min(b) } else { a.max(b) })
                }
                _ => return Err("Unsupported function or argument count".into()),
            }
        }
        "+" | "-" | "*" | "/" => {
            let a = eval_node(child("left")?, values, depth + 1, nodes)?;
            let b = eval_node(child("right")?, values, depth + 1, nodes)?;
            match (a, b) {
                (Some(a), Some(b)) => {
                    if op == "/" && b == 0. {
                        return Err("Division by zero".into());
                    }
                    Some(match op {
                        "+" => a + b,
                        "-" => a - b,
                        "*" => a * b,
                        _ => a / b,
                    })
                }
                _ => None,
            }
        }
        _ => return Err("Unsupported formula operator".into()),
    };
    if result.is_some_and(|v| !v.is_finite()) {
        return Err("Nonfinite formula result".into());
    }
    Ok(result)
}
pub fn evaluate_formula(ast: &str, values: &str) -> Result<String, String> {
    if ast.len() > 20000 || values.len() > 100000 {
        return Err(err("Formula input too large"));
    }
    let ast = serde_json::from_str(ast).map_err(err)?;
    let values = serde_json::from_str(values).map_err(err)?;
    serde_json::to_string(&eval_node(&ast, &values, 0, &mut 0).map_err(err)?).map_err(err)
}
fn normal_survival(z: f64) -> f64 {
    if z == f64::INFINITY {
        return 0.;
    }
    if z == f64::NEG_INFINITY {
        return 1.;
    }
    if z < 0. {
        return 1. - normal_survival(-z);
    }
    let t = 1. / (1. + 0.2316419 * z);
    let poly = t
        * (0.319381530
            + t * (-0.356563782 + t * (1.781477937 + t * (-1.821255978 + t * 1.330274429))));
    (-0.5 * z * z).exp() / (2. * std::f64::consts::PI).sqrt() * poly
}
fn interval_probability_impl(
    q05: f64,
    median: f64,
    q95: f64,
    lo: f64,
    hi: f64,
) -> Result<f64, String> {
    if ![q05, median, q95, lo, hi].iter().all(|v| v.is_finite())
        || !(q05 < median && median < q95)
        || lo > hi
    {
        return Err("Need finite bounds and q05 < median < q95".into());
    }
    if lo == hi {
        return Ok(0.);
    }
    let z = |x: f64| {
        (x - median)
            / (if x < median {
                median - q05
            } else {
                q95 - median
            } / 1.6448536269514722)
    };
    let (a, b) = (z(lo), z(hi));
    let p = if a >= 0. {
        normal_survival(a) - normal_survival(b)
    } else if b <= 0. {
        normal_survival(-b) - normal_survival(-a)
    } else {
        1. - normal_survival(b) - normal_survival(-a)
    };
    Ok(p.clamp(0., 1.))
}
pub fn interval_probability(
    q05: f64,
    median: f64,
    q95: f64,
    lo: f64,
    hi: f64,
) -> Result<f64, String> {
    interval_probability_impl(q05, median, q95, lo, hi).map_err(err)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn number(r: Vec<[f64; 2]>, presence: &str) -> Domain {
        Domain {
            ranges: Some(r),
            classes: None,
            presence: presence.into(),
        }
    }
    fn group(cols: Vec<usize>, counts: Vec<usize>, sum: Option<[f64; 2]>) -> Collection {
        Collection {
            name: "Budget".into(),
            columns: cols,
            counts,
            sum,
        }
    }
    #[test]
    fn disjoint_boxes_and_classes() {
        let req = Request {
            domains: vec![
                number(vec![[0., 1.], [9., 10.]], "required"),
                Domain {
                    ranges: None,
                    classes: Some(vec!["A".into()]),
                    presence: "optional".into(),
                },
            ],
            collections: vec![],
        };
        for r in sample_impl(&req, 512, 42).unwrap() {
            assert!(valid_row(&r, &req));
        }
    }
    #[test]
    fn mixed_empty_counts_and_sum() {
        let req = Request {
            domains: vec![number(vec![[0., 5.]], "optional"); 3],
            collections: vec![group(vec![0, 1, 2], vec![0, 2, 3], Some([0., 8.]))],
        };
        let rows = sample_impl(&req, 512, 8).unwrap();
        assert!(rows.iter().any(|r| r.iter().all(String::is_empty)));
        assert!(
            rows.iter()
                .any(|r| r.iter().filter(|s| !s.is_empty()).count() == 2)
        );
        for r in rows {
            assert!(valid_row(&r, &req));
        }
    }
    #[test]
    fn zero_is_selected() {
        let req = Request {
            domains: vec![
                number(vec![[0., 0.]], "required"),
                number(vec![[0., 0.]], "empty"),
            ],
            collections: vec![group(vec![0, 1], vec![1], Some([0., 0.]))],
        };
        assert_eq!(sample_impl(&req, 1, 1).unwrap()[0], vec!["0", ""]);
    }
    #[test]
    fn numeric_contradictions_are_not_claimed_exact_proofs() {
        let req = Request {
            domains: vec![number(vec![[0., 10.]], "required"); 2],
            collections: vec![
                group(vec![0, 1], vec![2], Some([2., 3.])),
                group(vec![0, 1], vec![2], Some([5., 6.])),
            ],
        };
        assert!(
            sample_impl(&req, 10, 1)
                .unwrap_err()
                .starts_with("No numeric witness")
        );
    }
    #[test]
    fn counts_conflict_with_required() {
        let req = Request {
            domains: vec![number(vec![[0., 10.]], "required"); 3],
            collections: vec![group(vec![0, 1, 2], vec![0, 2], None)],
        };
        assert!(
            sample_impl(&req, 1, 1)
                .unwrap_err()
                .starts_with("Infeasible")
        );
    }
    #[test]
    fn overlapping_collections() {
        let req = Request {
            domains: vec![number(vec![[0., 10.]], "optional"); 3],
            collections: vec![
                group(vec![0, 1], vec![1, 2], Some([3., 5.])),
                group(vec![1, 2], vec![1], Some([4., 6.])),
            ],
        };
        for row in sample_impl(&req, 128, 99).unwrap() {
            assert!(valid_row(&row, &req));
        }
    }
    #[test]
    fn count_zero_positive_sum_rejected() {
        let req = Request {
            domains: vec![number(vec![[1., 5.]], "optional"); 3],
            collections: vec![group(vec![0, 1, 2], vec![0], Some([1., 3.]))],
        };
        assert!(
            sample_impl(&req, 1, 1)
                .unwrap_err()
                .starts_with("No numeric witness")
        );
    }
    #[test]
    fn empty_categories_only() {
        let req = Request {
            domains: vec![Domain {
                ranges: None,
                classes: Some(vec![]),
                presence: "optional".into(),
            }],
            collections: vec![],
        };
        assert_eq!(sample_impl(&req, 1, 4).unwrap()[0], vec![""]);
    }
    #[test]
    fn partition_supports_match_exhaustive_bitmasks() {
        for seed in 0..80u32 {
            let n = 6;
            let mut rng = Rng(seed);
            let mut domains = vec![];
            for _ in 0..n {
                let mode = match (rng.next() * 5.) as usize {
                    0 => "required",
                    1 => "empty",
                    _ => "optional",
                };
                domains.push(number(vec![[0., 2.]], mode));
            }
            let groups = vec![
                group(
                    vec![0, 1, 2, 3],
                    (0..=4).filter(|_| rng.next() > 0.45).collect(),
                    None,
                ),
                group(
                    vec![2, 3, 4, 5],
                    (0..=4).filter(|_| rng.next() > 0.45).collect(),
                    None,
                ),
                group(vec![0, 2, 4], vec![0, 2, 3], None),
            ];
            if groups.iter().any(|g| g.counts.is_empty()) {
                continue;
            }
            let req = Request {
                domains,
                collections: groups,
            };
            let opts = validate(&req).unwrap();
            let actual = enumerate_supports(&opts, &req.collections, 200_000, 4096);
            assert!(actual.complete);
            let expected: std::collections::BTreeSet<Vec<bool>> = (0..(1usize << n))
                .map(|mask| (0..n).map(|i| mask & (1 << i) != 0).collect::<Vec<_>>())
                .filter(|s| {
                    req.domains
                        .iter()
                        .enumerate()
                        .all(|(i, d)| d.presence != "required" || s[i])
                        && req
                            .domains
                            .iter()
                            .enumerate()
                            .all(|(i, d)| d.presence != "empty" || !s[i])
                        && req.collections.iter().all(|g| {
                            g.counts
                                .contains(&g.columns.iter().filter(|&&i| s[i]).count())
                        })
                })
                .collect();
            assert_eq!(
                actual
                    .supports
                    .into_iter()
                    .collect::<std::collections::BTreeSet<_>>(),
                expected,
                "seed {seed}"
            );
        }
    }
    #[test]
    fn numeric_supports_match_brute_force_point_domains() {
        for seed in 0..24u32 {
            let req = Request {
                domains: vec![number(vec![[0., 0.], [2., 2.]], "optional"); 5],
                collections: vec![
                    group(vec![0, 1, 2], vec![0, 2, 3], Some([0., 4.])),
                    group(vec![1, 2, 3, 4], vec![2, 3], Some([2., 4.])),
                ],
            };
            let opts = validate(&req).unwrap();
            let (witnesses, diag) = catalog(&req, &opts, &mut Rng(seed)).unwrap();
            assert!(diag.support_enumeration_complete && diag.numeric_search_complete);
            let mut expected = std::collections::BTreeSet::new();
            for mut code in 0..3usize.pow(5) {
                let mut row = vec![];
                for _ in 0..5 {
                    row.push(
                        match code % 3 {
                            0 => "",
                            1 => "0",
                            _ => "2",
                        }
                        .to_string(),
                    );
                    code /= 3;
                }
                if valid_row(&row, &req) {
                    expected.insert(row.iter().map(|s| !s.is_empty()).collect::<Vec<_>>());
                }
            }
            assert_eq!(
                witnesses
                    .into_iter()
                    .map(|w| w.support)
                    .collect::<std::collections::BTreeSet<_>>(),
                expected
            );
        }
    }
    #[test]
    fn same_membership_different_domains_keep_distinct_identities() {
        let req = Request {
            domains: vec![
                number(vec![[1., 1.]], "optional"),
                number(vec![[9., 9.]], "optional"),
            ],
            collections: vec![group(vec![0, 1], vec![1], Some([9., 9.]))],
        };
        let r = sample_result(&req, 5, 1).unwrap();
        assert_eq!(r.diagnostics.partitions, 1);
        assert_eq!(r.diagnostics.count_valid_supports_enumerated, 2);
        assert_eq!(r.diagnostics.supports_with_numeric_witness, 1);
        assert!(
            r.rows
                .iter()
                .all(|r| r == &vec!["".to_string(), "9".to_string()])
        );
    }
    #[test]
    fn capped_enumeration_never_claims_complete() {
        let opts = vec![vec![Choice::Empty, Choice::Numeric(0., 1.)]; 20];
        let result = enumerate_supports(&opts, &[], 200_000, 8);
        assert!(!result.complete);
        assert_eq!(result.supports.len(), 8);
        let exhausted = enumerate_supports(&opts, &[], 1, 4096);
        assert!(!exhausted.complete);
        assert!(exhausted.supports.is_empty());
    }
    #[test]
    fn all_small_feasible_supports_represented_when_batch_large_enough() {
        let req = Request {
            domains: vec![number(vec![[0., 5.]], "optional"); 4],
            collections: vec![
                group(vec![0, 1, 2], vec![0, 2], None),
                group(vec![1, 2, 3], vec![1, 2], None),
            ],
        };
        let result = sample_result(&req, 64, 33).unwrap();
        assert!(result.diagnostics.support_enumeration_complete);
        assert_eq!(
            result.diagnostics.distinct_supports_sampled,
            result.diagnostics.supports_with_numeric_witness
        );
    }

    #[test]
    fn evolution_preserves_overlap_and_rejects_bad_parents() {
        let req = Request {
            domains: vec![number(vec![[0., 3.], [6., 9.]], "optional"); 4],
            collections: vec![
                group(vec![0, 1, 2], vec![0, 2, 3], Some([0., 12.])),
                group(vec![1, 2, 3], vec![1, 2], Some([2., 10.])),
            ],
        };
        let mut parents = sample_impl(&req, 24, 9).unwrap();
        parents.push(vec!["999".into(); 4]);
        parents.push(vec!["bad".into()]);
        for seed in 0..15 {
            let r = evolve_result(&req, &parents, 64, seed).unwrap();
            assert_eq!(r.rejected_parents, 2);
            assert!(r.rows.iter().all(|row| valid_row(row, &req)));
            assert!(r.crossover_children > 0);
            assert!(r.explorers > 0);
        }
    }
    #[test]
    fn evolution_respects_categories_and_zero() {
        let req = Request {
            domains: vec![
                number(vec![[0., 0.]], "optional"),
                Domain {
                    ranges: None,
                    classes: Some(vec!["email".into(), "search".into()]),
                    presence: "optional".into(),
                },
            ],
            collections: vec![group(vec![0, 1], vec![0, 2], None)],
        };
        let parents = sample_impl(&req, 16, 1).unwrap();
        let r = evolve_result(&req, &parents, 64, 12).unwrap();
        assert!(r.rows.iter().all(|row| valid_row(row, &req)));
        assert!(r.rows.iter().any(|row| row[0] == "0"));
    }

    #[test]
    fn rust_formula_missing_zero_and_errors() {
        let ast = serde_json::json!({"op":"call","name":"coalesce","args":[{"op":"ref","name":"x"},{"op":"number","v":0}]});
        assert_eq!(
            eval_node(&ast, &serde_json::json!({"x":""}), 0, &mut 0).unwrap(),
            Some(0.)
        );
        let invalid = serde_json::json!({"op":"/","left":{"op":"number","v":1},"right":{"op":"number","v":0}});
        assert!(eval_node(&invalid, &serde_json::json!({}), 0, &mut 0).is_err());
    }
    #[test]
    fn rust_probabilities_have_extrapolated_tails() {
        let p = interval_probability_impl(-1.644853627, 0., 1.644853627, -1.644853627, 1.644853627)
            .unwrap();
        assert!((p - 0.9).abs() < 1e-6);
        assert!(interval_probability_impl(-1., 0., 1., 2., 3.).unwrap() > 0.);
        assert!(interval_probability_impl(0., 0., 0., 1., 2.).is_err());
        assert_eq!(
            interval_probability_impl(-1., 0., 1., 1e6, 2e6).unwrap(),
            0.
        );
    }
}
