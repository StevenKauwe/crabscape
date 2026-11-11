use dragon_game::{abilities::*, combat::Path, flight::Player, projectiles::Target};
use serde_json::{Value, json};
fn targets() -> Vec<Target> {
    (0..10)
        .map(|id| Target {
            id,
            x: 100.0 + (id as f64 - 4.0) * 50.0,
            y: 200.0,
            r: 20.0,
            boss: id == 3,
            dead: id == 8,
            calm: id == 9,
            ..Target::default()
        })
        .collect()
}
fn player() -> Player {
    Player {
        x: 100.0,
        y: 200.0,
        hearts: 3,
        ..Player::default()
    }
}
fn state(p: &Player, events: &[Event]) -> Value {
    json!({"energy":p.energy,"shield":p.shield,"hearts":p.hearts,"safe":p.safe,"shadow":p.shadow,"specialCd":p.special_cd,"megaT":p.mega_t,"magnet":p.magnet,"sila":p.sila,"tired":p.tired,"dadCd":p.dad_cd,"events":events.iter().map(|e|match e {Event::Hit{id,shot:s}=>json!(["hit",id,s.dmg,s.stun,s.mark,s.crack,s.mega]),Event::Calm{id}=>json!(["calm",id]),Event::GemRing{count,rich}=>json!(["gems",count,rich]),Event::Cast{key}=>json!(["cast",key]),Event::Rain{duration}=>json!(["rain",duration]),Event::TooTired=>json!(["tired"])}).collect::<Vec<_>>()})
}
#[test]
fn gates_and_overwrites_match_legacy() {
    let mut p = player();
    let r = Rules::default();
    p.energy = 49.0;
    assert_eq!(
        cast_spell(&mut p, &r, &[], Spell::Naina),
        vec![Event::TooTired]
    );
    assert_eq!(p.energy, 49.0);
    p.down = 2.0;
    assert!(
        !do_special(&mut p, &r, &targets()).is_empty(),
        "special has no down gate"
    );
    assert!(do_special(&mut p, &r, &targets()).is_empty());
    p.shield = 30.0;
    apply_spell(&mut p, &r, &[], Spell::Thuri);
    assert_eq!(p.shield, 6.0);
    let mut r = Rules { daddio: true, ..r };
    assert!(dad_power(&mut p, &r, &targets(), DadPower::Said, || panic!()).is_empty());
    r.active_words = vec![Spell::Thuri];
    p.energy = 100.0;
    assert!(
        !cast_spell(&mut p, &r, &[], Spell::Thuri).is_empty(),
        "spell has no down gate"
    );
}
#[test]
fn loadout_order_duplicates_and_unlocks_are_preserved() {
    let cleared = vec!["m2".into(), "m3".into(), "m5".into()];
    assert_eq!(active_words(None), vec![]);
    assert_eq!(
        active_words(Some((&cleared, &[Spell::Faro], 0))),
        vec![Spell::Naina, Spell::Thuri]
    );
    assert_eq!(
        active_words(Some((
            &cleared,
            &[Spell::Vara, Spell::Vara, Spell::Kaelo],
            3
        ))),
        vec![Spell::Vara, Spell::Vara, Spell::Kaelo]
    );
    assert_eq!(Spell::Thuri.cost(2), 44.0);
    assert_eq!(max_energy(4), 130.0);
}
#[test]
fn target_selection_and_snapshot_commands_are_exact() {
    let mut p = player();
    let r = Rules {
        path: Path::Storm,
        rider_skill: 2,
        bellows: 2.0,
        ..Rules::default()
    };
    let events = do_special(&mut p, &r, &targets());
    let ids: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let Event::Hit { id, shot } = e {
                assert!((shot.dmg - 5.6).abs() < 1e-12);
                Some(*id)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(ids, vec![4, 3, 5, 2, 6, 1]);
    let mut ts = targets();
    ts[0].x = p.x;
    ts[0].dead = true;
    assert!(
        apply_spell(&mut p, &r, &ts, Spell::Vara).contains(&Event::Calm { id: 0 }),
        "vara delegates dead/calm guard to defeat"
    );
    assert_eq!(mega_blast(&ts).len(), 7);
    let mut draws = 0;
    let r = Rules { daddio: true, ..r };
    let e = dad_power(&mut p, &r, &ts, DadPower::Joke, || {
        draws += 1;
        0.5
    });
    assert_eq!(draws, 1);
    assert_eq!(
        e.last(),
        Some(&Event::Cast {
            key: "joke:4".into()
        })
    );
}
fn block(start: &str, end: &str) -> &'static str {
    let s = include_str!("../../../web/dragon/index.html");
    let tail = &s[s.find(start).unwrap()..];
    &tail[..tail.find(end).unwrap()]
}
fn compare(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert!(
            (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 1e-9,
            "{a} != {b}"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                compare(a, b)
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (k, v) in a {
                compare(v, &b[k]);
            }
        }
        _ => assert_eq!(a, b),
    }
}
#[test]
#[ignore = "requires macOS JavaScriptCore CLI; run explicitly on a Mac"]
fn original_source_matches_every_ability_and_skill_level() {
    let mut cases = vec![];
    let mut expected = vec![];
    for skill in 0..=5 {
        for action in [
            "shadow", "forge", "storm", "guardian", "mega", "burst", "blast", "said", "joke",
            "naina", "thuri", "kaelo", "vara", "sila", "mira", "senn", "faro",
        ] {
            let mut p = player();
            let r = Rules {
                rider_skill: skill,
                magic_skill: skill,
                bellows: 2.0,
                scale: 40.0,
                max_hearts: 7,
                daddio: matches!(action, "mega" | "said" | "joke"),
                path: match action {
                    "shadow" => Path::Shadow,
                    "storm" => Path::Storm,
                    "guardian" => Path::Guardian,
                    _ => Path::Forge,
                },
                active_words: Spell::ALL.to_vec(),
                ..Rules::default()
            };
            let ts = targets();
            let events = match action {
                "shadow" | "forge" | "storm" | "guardian" | "mega" => do_special(&mut p, &r, &ts),
                "burst" => shadow_burst(&p, &r, &ts),
                "blast" => mega_blast(&ts),
                "said" | "joke" => dad_power(
                    &mut p,
                    &r,
                    &ts,
                    if action == "said" {
                        DadPower::Said
                    } else {
                        DadPower::Joke
                    },
                    || 0.5,
                ),
                _ => cast_spell(
                    &mut p,
                    &r,
                    &ts,
                    Spell::ALL.into_iter().find(|s| s.key() == action).unwrap(),
                ),
            };
            cases.push(json!({"skill":skill,"action":action}));
            expected.push(state(&p, &events));
        }
    }
    let script = format!(
        r#"
const cases={cases}, fixtures={targets};
let P,R,me,skill,events,castCount,lastCast,rain;
const S=40, W=800, topLimit=20, groundY=500, MEGA_DELAY=.6, state='play', reduceMotion=true;
let enemies,shake; const rings=[],texts=[],particles=[];
const sfx=new Proxy({{}},{{get:()=>()=>{{}}}});
const isDaddio=()=>R.dad, skillLv=()=>skill, bellowsMult=()=>1.4, maxHearts=()=>7;
const specialCdMax=()=>R.dad?30:skill>=3?9:12;
const spellPower=()=>skill>=5?1.5:1, spellCost=k=>Math.round(WORDS[k].cost*(skill>=2?.8:1));
const activeWords=()=>Object.keys(WORDS),dist=(x,y,a,b)=>Math.hypot(x-a,y-b);
const plog=()=>{{}},epicMega=()=>{{}},epicSaid=()=>{{}},epicJoke=()=>{{}},toast=()=>{{}},burst=()=>{{}},bolt=()=>{{}},updateHud=()=>{{}},megaBoomFx=()=>{{}},megaSpot=()=>({{x:0,y:0}});
const rand=(a,b)=>(a+b)/2,pick=a=>a[0];
const hitEnemy=(e,s)=>events.push(['hit',e.id,s.dmg,s.stun||0,!!s.mark,!!s.crack,!!s.mega]);
const defeat=e=>events.push(['calm',e.id]);const gemRing=(n,rich=false)=>events.push(['gems',n,rich]);
{words}
{dad}
{area}
{special}
{shadow}
{powers}
{cast}
{apply}
{mega}
const results=cases.map(c=>{{skill=c.skill;P={{x:100,y:200,energy:100,shield:0,hearts:3,safe:0,shadow:0,specialCd:0,megaT:0,magnet:0,sila:0,tired:0,dadCd:{{}},down:0,dizzy:0,out:false,facing:1}};R={{path:c.action,dad:['mega','said','joke'].includes(c.action)}};me={{}};enemies=fixtures;events=[];castCount=0;lastCast='';rain=0;
if(['shadow','forge','storm','guardian','mega'].includes(c.action))doSpecial();else if(c.action==='burst')shadowBurst();else if(c.action==='blast')megaBlast();else if(['said','joke'].includes(c.action))dadPower(c.action);else castSpell(c.action);
if(rain)events.unshift(['rain',rain]);if(castCount)events.push(['cast',lastCast]);delete P.x;delete P.y;delete P.down;delete P.dizzy;delete P.out;delete P.facing;return {{...P,events}};}});print(JSON.stringify(results));
"#,
        cases = serde_json::to_string(&cases).unwrap(),
        targets = serde_json::to_string(&targets()).unwrap(),
        words = format_args!("{}\n}};", block("const WORDS = {", "\n};")),
        dad = block("const DAD_POWERS = {", "const STAGES"),
        area = block("function damageArea(", "\nfunction "),
        special = block("function doSpecial()", "// A lightning bolt"),
        shadow = block("function shadowBurst()", "/* ---------- The Old Tongue"),
        powers = block("function dadPower(", "function castSpell"),
        cast = block("function castSpell(", "function applySpell"),
        apply = block("function applySpell(", "/* ---------- Eggs"),
        mega = block("function megaBlast()", "function megaBoomFx")
    );
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut child =
        Command::new("/System/Library/Frameworks/JavaScriptCore.framework/Versions/A/Helpers/jsc")
            .args(["-e", "eval(JSON.parse(readline()))"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{}\n", serde_json::to_string(&script).unwrap()).as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let actual: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(expected.len(), actual.len());
    for (i, (a, b)) in expected.iter().zip(&actual).enumerate() {
        eprintln!("{}", cases[i]);
        compare(a, b);
    }
}
