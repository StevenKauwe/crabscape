//! Repeatable real-model acceptance run. Usage: royal_trial /path/to/original/weights
use king_ddd::{Band, BandMode, Book, Dataset, Role, TournamentConfig, WorkRequest};
fn main() -> Result<(), String> {
    let root = std::env::args()
        .nth(1)
        .ok_or("Pass the model weights directory")?;
    let book = Book::new(
        include_bytes!("../../../assets/king-ddd/launch-quest.xlsx"),
        true,
    )?;
    let s: serde_json::Value =
        serde_json::from_str(&book.snapshot(0)?).map_err(|e| e.to_string())?;
    let mut rows: Vec<Vec<String>> =
        serde_json::from_value(s["data"].clone()).map_err(|e| e.to_string())?;
    let headers = rows.remove(0);
    let dataset = Dataset { headers, rows };
    let mut columns = dataset.infer_columns();
    for c in &mut columns {
        if c.name == "campaign_id" {
            c.role = Role::Id;
        }
        if ["new_customers", "delivery_cost_k", "csat"].contains(&c.name.as_str()) {
            c.role = Role::Output;
        }
    }
    if let Some(c) = columns.iter_mut().find(|c| c.name == "new_customers") {
        c.goal = Some(Band {
            mode: BandMode::AtLeast,
            lo: 300.,
            hi: 1000.,
        });
    }
    let request = WorkRequest {
        dataset,
        columns,
        collections: vec![],
        tournament: TournamentConfig {
            population: 8,
            generations: 2,
            ..Default::default()
        },
    };
    let mut bytes = vec![];
    for i in 0..4 {
        bytes.extend(
            std::fs::read(format!("{root}/regressor-v2-half.{i:02}.bin"))
                .map_err(|e| e.to_string())?,
        );
    }
    let result = king_ddd::run(&request, Some(bytes), None, |g| {
        eprintln!(
            "generation={} judged={} reaped={} best={:?}",
            g.generation, g.judged, g.reaped, g.best
        )
    })?;
    assert_eq!(result.judged, 16);
    assert_eq!(result.history.len(), 2);
    assert!(!result.archive.is_empty());
    assert!(
        result
            .archive
            .iter()
            .all(|c| c.predictions.len() == 3 && c.hit.is_some_and(|p| p.is_finite()))
    );
    println!(
        "{}",
        serde_json::to_string(&result).map_err(|e| e.to_string())?
    );
    Ok(())
}
