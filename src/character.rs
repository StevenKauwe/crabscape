//! Taking your crab to another browser, device or build of the game: on the
//! settings panel's Game tab, "Your character" has Export and Import.
//!
//! Export hands the crab over as a character code, a line of text, two ways
//! at once: copied, for pasting into a message or a note, and as a `.crab`
//! file holding the same text (on a phone through the share sheet, where the
//! browser can share files; otherwise a download). Import takes either,
//! pasted or picked, says what it's about to load (skills, eggs eaten, where
//! the crab is, which build made it) and asks "Replace your crab?" before
//! anything is overwritten. Then the page reloads into the imported crab,
//! which also leaves any co-op game the way a reload always does.
//!
//! A code is `crab:<format>:<body>:<checksum>`:
//! - `format`: the layout's version, [`FORMAT`]. A higher one is refused as
//!   "made by a newer version of the game".
//! - `body`: JSON, deflated (a crab's save shrinks about 4 times), in
//!   URL-safe base64 without padding (letters, digits, `-` and `_`, which
//!   messages and notes leave alone): the build that made it, the whole save
//!   (`save::SaveData`: where the crab is, what it remembers, what it grew and
//!   took, its own world), the settings, the art style and the name the
//!   player goes by (`names.rs`; codes from before names have none).
//! - `checksum`: CRC-32 of everything before it, as 8 hex digits, so a code
//!   damaged or cut short on the way is refused instead of loaded half.
//!
//! Spaces and line breaks a message adds are ignored. Anything else that
//! isn't a whole, unchanged code is refused with a plain message, never a
//! crash. Every build reads the same codes, so a crab moves between a preview
//! build and the live game both ways.
//!
//! What travels: the crab and its own world (the whole save), and the
//! player's settings: volumes, music, art style, how friends' voices carry,
//! and the name over the crab.
//! What stays with the device: the co-op lobby (its invite link: lobbies are
//! per device), the battery saver (it's about this device's battery), the
//! settings tab last open and the feel lab's preset (a tool for previews).
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use bevy::prelude::*;
use crabsim::net::clean_name;
use crabsim::{PlayerId, Sim, Skill, skills};
use serde::{Deserialize, Serialize};

use crate::save::{SAVE_VERSION, SaveData};
use crate::settings::Settings;
use crate::style::ArtStyle;

/// The code layout's version: bump it when the layout changes, and keep
/// reading the old ones.
pub const FORMAT: u32 = 1;

/// Every code starts with this.
const PREFIX: &str = "crab:";

/// The longest text the importer looks at (1 MiB), and the most JSON it
/// unpacks (8 MiB). A crab that has been a few minutes out is about 1.5 KB.
pub const MAX_TEXT: usize = 1 << 20;
const MAX_JSON: usize = 8 << 20;

/// A crab, ready to travel.
#[derive(Clone, Debug, PartialEq)]
pub struct Character {
    /// The build that made it ("5d3eb47, Oct 29", or "dev").
    pub build: String,
    /// Which copy of the game: a preview's folder ("pr-183",
    /// "branches-my-idea"), or None for the live game.
    pub copy: Option<String>,
    pub save: SaveData,
    pub settings: Settings,
    pub style: ArtStyle,
    /// The name the player goes by (`names.rs`), if the code carries one:
    /// codes from before names don't, and importing one keeps the name here.
    pub name: Option<String>,
}

impl Character {
    /// Crab `id` as it is right now, with these settings.
    pub fn of(
        sim: &Sim,
        id: PlayerId,
        settings: Settings,
        style: ArtStyle,
        name: &str,
        build: &str,
        copy: Option<String>,
    ) -> Option<Self> {
        Some(Self {
            build: build.to_string(),
            copy,
            save: crate::save::snapshot(sim, id)?,
            settings,
            style,
            name: clean_name(name),
        })
    }
}

/// The JSON inside a code. The save is read as plain JSON first, so its
/// version can be checked before its fields are.
#[derive(Serialize, Deserialize)]
struct Body<S> {
    build: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    copy: Option<String>,
    save: S,
    /// As `Settings::to_text` keeps them.
    #[serde(default)]
    settings: String,
    #[serde(default)]
    style: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

/// Why a code wasn't taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Not a code at all.
    NotACharacter,
    /// A code, but changed or cut short on the way.
    Damaged,
    /// Made by a newer game than this one.
    Newer,
    /// From a save this game no longer reads.
    TooOld,
    /// Far bigger than any crab.
    TooBig,
}

impl Refusal {
    /// What the player is told.
    pub fn words(self) -> &'static str {
        match self {
            Refusal::NotACharacter => "That isn't a CrabScape character.",
            Refusal::Damaged => {
                "That character is damaged or cut short. Copy all of it and try again."
            }
            Refusal::Newer => {
                "That character was made by a newer version of the game. Reload the page to get \
                 it, then try again."
            }
            Refusal::TooOld => "That character is from an old version of the game it can't read.",
            Refusal::TooBig => "That's far too big to be a CrabScape character.",
        }
    }
}

/// The character as a code (see the top of this file).
pub fn encode(c: &Character) -> String {
    let body = Body {
        build: c.build.clone(),
        copy: c.copy.clone(),
        save: &c.save,
        settings: c.settings.to_text(),
        style: c.style.name().to_string(),
        name: c.name.clone(),
    };
    let json = serde_json::to_vec(&body).expect("a character always serialises");
    let packed = miniz_oxide::deflate::compress_to_vec(&json, 9);
    let head = format!("{PREFIX}{FORMAT}:{}", base64url(&packed));
    let sum = checksum(&head);
    format!("{head}:{sum}")
}

/// A code (or a `.crab` file's text) back into a character, or why not.
pub fn decode(text: &str) -> Result<Character, Refusal> {
    if text.len() > MAX_TEXT {
        return Err(Refusal::TooBig);
    }
    let clean: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{feff}')
        .collect();
    let rest = clean.strip_prefix(PREFIX).ok_or(Refusal::NotACharacter)?;
    let mut parts = rest.split(':');
    let format = parts
        .next()
        .and_then(|f| f.parse::<u64>().ok())
        .filter(|&f| f > 0)
        .ok_or(Refusal::NotACharacter)?;
    if format > u64::from(FORMAT) {
        return Err(Refusal::Newer);
    }
    let (Some(body), Some(sum), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(Refusal::Damaged);
    };
    let head = &clean[..clean.len() - sum.len() - 1];
    if !sum.eq_ignore_ascii_case(&checksum(head)) {
        return Err(Refusal::Damaged);
    }
    // Intact (the checksum says so) but not packed as a character is.
    let packed = unbase64url(body).ok_or(Refusal::NotACharacter)?;
    let json = miniz_oxide::inflate::decompress_to_vec_with_limit(&packed, MAX_JSON).map_err(
        |e| match e.status {
            miniz_oxide::inflate::TINFLStatus::HasMoreOutput => Refusal::TooBig,
            _ => Refusal::NotACharacter,
        },
    )?;
    let body: Body<serde_json::Value> =
        serde_json::from_slice(&json).map_err(|_| Refusal::NotACharacter)?;
    let version = body
        .save
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(Refusal::NotACharacter)?;
    if version > u64::from(SAVE_VERSION) {
        return Err(Refusal::Newer);
    }
    if version < u64::from(SAVE_VERSION) {
        return Err(Refusal::TooOld);
    }
    let save: SaveData = serde_json::from_value(body.save).map_err(|_| Refusal::NotACharacter)?;
    if !save.x.is_finite() || !save.y.is_finite() {
        return Err(Refusal::NotACharacter);
    }
    Ok(Character {
        build: body.build,
        copy: body.copy,
        save,
        settings: Settings::from_text(&body.settings),
        style: ArtStyle::from_name(&body.style).unwrap_or_default(),
        name: body.name.as_deref().and_then(clean_name),
    })
}

/// What importing `c` writes, as `save::replace` takes it: the save, the
/// settings with this device's battery saver (`device_settings`: the
/// settings kept here now), and the art style. Nothing else is touched, the
/// co-op lobby among it.
pub fn writes(c: &Character, device_settings: Option<&str>) -> Vec<(&'static str, String)> {
    let saver = device_settings
        .map(Settings::from_text)
        .unwrap_or_default()
        .saver;
    let settings = Settings {
        saver,
        ..c.settings
    };
    let mut keys = vec![
        (
            ".save",
            serde_json::to_string(&c.save).expect("save data always serialises"),
        ),
        ("-settings", settings.to_text()),
        ("-style", c.style.name().to_string()),
    ];
    if let Some(name) = &c.name {
        keys.push((crate::names::KEPT, name.clone()));
    }
    keys
}

/// What the importer shows before asking "Replace your crab?": its skills,
/// eggs eaten and lantern, where it is, and where it came from.
pub fn summary(c: &Character, sim: &Sim) -> String {
    let p = c.save.progress.clone().unwrap_or_default();
    let skills = Skill::ALL
        .map(|s| {
            let xp = match s {
                Skill::Climbing => p.climb_xp,
                Skill::Swimming => p.swim_xp,
                Skill::Fighting => p.fight_xp,
            };
            format!("{} {}", s.name(), skills::level(xp, s.step(&sim.tuning)))
        })
        .join(", ");
    let eaten = p.eaten.iter().filter(|e| **e).count();
    let cultures = match p.lantern {
        1 => "1 culture".to_string(),
        n => format!("{n} cultures"),
    };
    let away =
        Vec2::new(c.save.x, c.save.y).distance(sim.level.home_den) / crate::deep::UNITS_PER_METRE;
    let place = if away < 1.0 {
        "At home".to_string()
    } else {
        format!("{away:.0} m from home")
    };
    let from = match c.copy.as_deref() {
        None => "the live game".to_string(),
        Some(s) => match s.strip_prefix("branches-") {
            Some(branch) => format!("branch {branch}"),
            None => format!("preview {s}"),
        },
    };
    format!(
        "{skills}\n{eaten} of {} eggs eaten, {cultures} in its lantern\n{place}\nFrom {from}, build \
         {}",
        sim.level.eggs.len(),
        c.build
    )
}

/// The exported file's name: `crabscape-2026-10-29.crab`, with the preview
/// in it for a copy that isn't the live game (`crabscape-pr-183-...`).
pub fn file_name(date: (u32, u32, u32), copy: Option<&str>) -> String {
    let (year, month, day) = date;
    let copy = copy.map(|c| format!("{c}-")).unwrap_or_default();
    format!("crabscape-{copy}{year:04}-{month:02}-{day:02}.crab")
}

/// CRC-32 as 8 hex digits.
fn checksum(text: &str) -> String {
    format!("{:08x}", crc32fast::hash(text.as_bytes()))
}

const B64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// URL-safe base64, without padding.
fn base64url(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        // The chunk's bytes as one number, the first byte highest.
        let n = chunk.iter().fold(0u32, |n, &b| (n << 8) + u32::from(b)) << (8 * (3 - chunk.len()));
        for k in 0..=chunk.len() {
            s.push(B64URL[(n >> (18 - 6 * k) & 63) as usize] as char);
        }
    }
    s
}

/// A base64 letter's value.
fn sextet(c: u8) -> Option<u32> {
    let v = match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'-' => 62,
        b'_' => 63,
        _ => return None,
    };
    Some(u32::from(v))
}

/// Back from [`base64url`]; None for anything it can't have written.
fn unbase64url(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    for chunk in s.as_bytes().chunks(4) {
        if chunk.len() == 1 {
            return None;
        }
        let mut n = 0u32;
        for (k, &c) in chunk.iter().enumerate() {
            n |= sextet(c)? << (18 - 6 * k);
        }
        for k in 0..chunk.len() - 1 {
            out.push((n >> (16 - 8 * k)) as u8);
        }
    }
    Some(out)
}

pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, _app: &mut App) {
        #[cfg(target_arch = "wasm32")]
        _app.add_systems(
            Update,
            (web::export_when_asked, web::check_import).in_set(crate::player::GameSet::Present),
        );
    }
}

#[cfg(target_arch = "wasm32")]
pub use web::{ask_export, ask_import};

/// The page's character box (`#character` in `web/index.html`): the code and
/// its buttons for Export, the paste box, file picker and "Replace your
/// crab?" for Import. The canvas's `data-character` says what it's doing, for
/// the browser tests: "exported <length>", "import", "ready", "refused
/// <why>", "replacing" or "closed".
#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::RefCell;

    use bevy::prelude::*;
    use crabsim::Sim;
    use js_sys::{Array, Promise, Reflect};
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::{JsFuture, spawn_local};
    use web_sys::{
        Document, File, FilePropertyBag, HtmlAnchorElement, HtmlDocument, HtmlElement,
        HtmlInputElement, HtmlTextAreaElement, Navigator, ShareData, Url,
    };

    use super::{Character, MAX_TEXT, Refusal, decode, encode, file_name, summary, writes};
    use crate::coop::LocalId;
    use crate::settings::Settings;
    use crate::style::ArtStyle;

    /// Said once the code is copied.
    const COPIED: &str = "Copied. Paste it into a message or a note, or save it as a file.";
    /// Said when the import box opens or is emptied.
    const PASTE: &str = "Paste a character code, or choose a .crab file.";

    #[derive(Default)]
    struct Dialog {
        /// Asked for by the settings panel; done next frame.
        export_asked: bool,
        import_asked: bool,
        wired: bool,
        /// Import (not export) is showing.
        importing: bool,
        /// Text to check (pasted, typed or from a file).
        pasted: Option<String>,
        /// Checked, and ready to replace the crab.
        ready: Option<Character>,
        /// The exported code, its file's name, and the last download's
        /// address (let go at the next one).
        code: String,
        file: String,
        url: Option<String>,
    }

    thread_local! {
        static DIALOG: RefCell<Dialog> = RefCell::default();
    }

    fn with<R>(f: impl FnOnce(&mut Dialog) -> R) -> R {
        DIALOG.with(|d| f(&mut d.borrow_mut()))
    }

    /// Show the crab's code (the settings panel's Export).
    pub fn ask_export() {
        with(|d| d.export_asked = true);
    }

    /// Open the import box (the settings panel's Import).
    pub fn ask_import() {
        with(|d| d.import_asked = true);
    }

    fn document() -> Option<Document> {
        web_sys::window()?.document()
    }

    fn element<T: JsCast>(id: &str) -> Option<T> {
        document()?.get_element_by_id(id)?.dyn_into().ok()
    }

    fn set_text(id: &str, words: &str) {
        if let Some(e) = element::<HtmlElement>(id) {
            e.set_text_content(Some(words));
        }
    }

    fn note(words: &str) {
        set_text("character-note", words);
    }

    fn set_hidden(id: &str, hidden: bool) {
        if let Some(e) = element::<HtmlElement>(id) {
            e.set_hidden(hidden);
        }
    }

    fn text_box() -> Option<HtmlTextAreaElement> {
        element("character-code")
    }

    fn tell_tests(state: &str) {
        if let Some(c) = element::<HtmlElement>("crabscape") {
            let _ = c.set_attribute("data-character", state);
        }
    }

    /// The box, for exporting or importing.
    fn open(importing: bool) {
        wire();
        set_text(
            "character-title",
            if importing {
                "Import character"
            } else {
                "Export character"
            },
        );
        set_text("character-summary", "");
        set_hidden("character-export", importing);
        set_hidden("character-import", !importing);
        set_hidden("character-confirm", true);
        if let Some(t) = text_box() {
            t.set_value("");
            t.set_read_only(!importing);
        }
        with(|d| {
            d.importing = importing;
            d.pasted = None;
            d.ready = None;
        });
        set_hidden("character", false);
    }

    fn close() {
        set_hidden("character", true);
        with(|d| {
            d.ready = None;
            d.pasted = None;
        });
        tell_tests("closed");
        if let Some(c) = element::<HtmlElement>("crabscape") {
            let _ = c.focus();
        }
    }

    /// Export: the crab as it is now, as a code in the box, copied.
    pub fn export_when_asked(
        sim: Res<Sim>,
        me: Res<LocalId>,
        settings: Res<Settings>,
        style: Res<ArtStyle>,
        names: Res<crate::names::Names>,
    ) {
        if !with(|d| std::mem::take(&mut d.export_asked)) {
            return;
        }
        let build = option_env!("CRABSCAPE_BUILD").unwrap_or("dev");
        let copy = crate::save::build_slug();
        let name = names.mine();
        let Some(c) = Character::of(&sim, me.0, *settings, *style, name, build, copy.clone())
        else {
            return;
        };
        let code = encode(&c);
        let today = js_sys::Date::new_0();
        let date = (
            today.get_full_year(),
            today.get_month() + 1,
            today.get_date(),
        );
        open(false);
        set_text("character-summary", &summary(&c, &sim));
        if let Some(t) = text_box() {
            t.set_value(&code);
        }
        info!("export: a character code of {} letters", code.len());
        tell_tests(&format!("exported {}", code.len()));
        with(|d| {
            d.file = file_name(date, copy.as_deref());
            d.code = code;
        });
        // Not from a tap on the page itself, so some browsers refuse: then
        // the Copy button does it.
        match start_copy() {
            Some(copying) => spawn_local(async move {
                note(if JsFuture::from(copying).await.is_ok() {
                    COPIED
                } else {
                    "Tap Copy code to copy it, or Save file."
                });
            }),
            None => note("Tap Copy code to copy it, or Save file."),
        }
    }

    fn navigator() -> Option<Navigator> {
        Some(web_sys::window()?.navigator())
    }

    /// Does the browser have `navigator.<name>`? (Calling one it lacks
    /// would stop the game.)
    fn has(nav: &Navigator, name: &str) -> bool {
        Reflect::get(nav, &name.into()).is_ok_and(|v| !v.is_undefined())
    }

    /// Start copying the exported code (inside a tap, for browsers that
    /// insist on one).
    fn start_copy() -> Option<Promise> {
        let nav = navigator()?;
        has(&nav, "clipboard").then(|| nav.clipboard().write_text(&with(|d| d.code.clone())))
    }

    /// The Copy code button: the clipboard, or for a browser without one,
    /// the old way (select the box, copy).
    fn copy() {
        if let Some(copying) = start_copy() {
            spawn_local(async move {
                if JsFuture::from(copying).await.is_ok() {
                    note(COPIED);
                } else {
                    copy_selected();
                }
            });
        } else {
            copy_selected();
        }
    }

    fn copy_selected() {
        if let Some(t) = text_box() {
            t.select();
        }
        let copied = document()
            .and_then(|d| d.dyn_into::<HtmlDocument>().ok())
            .and_then(|d| d.exec_command("copy").ok())
            .unwrap_or(false);
        note(if copied {
            COPIED
        } else {
            "Couldn't copy here: select the code and copy it yourself, or Save file."
        });
    }

    /// The Save file button: the share sheet on a phone that can share
    /// files, otherwise a download.
    fn save_file() {
        let (code, name) = with(|d| (d.code.clone(), d.file.clone()));
        let options = FilePropertyBag::new();
        options.set_type("text/plain");
        let Ok(file) =
            File::new_with_str_sequence_and_options(&Array::of1(&code.into()), &name, &options)
        else {
            return;
        };
        if let Some(nav) = navigator()
            && nav.max_touch_points() > 0
            && has(&nav, "canShare")
            && has(&nav, "share")
        {
            let data = ShareData::new();
            data.set_files(&Array::of1(&file));
            data.set_title("CrabScape character");
            if nav.can_share_with_data(&data) {
                let sharing = nav.share_with_data(&data);
                spawn_local(async move {
                    match JsFuture::from(sharing).await {
                        Ok(_) => note("Saved."),
                        // Closed the sheet: nothing to say.
                        Err(e) if error_name(&e) == "AbortError" => {}
                        Err(_) => download(&file, &name),
                    }
                });
                return;
            }
        }
        download(&file, &name);
    }

    fn error_name(e: &JsValue) -> String {
        Reflect::get(e, &"name".into())
            .ok()
            .and_then(|n| n.as_string())
            .unwrap_or_default()
    }

    fn download(file: &File, name: &str) {
        let Ok(url) = Url::create_object_url_with_blob(file) else {
            return;
        };
        let Some(link) = document()
            .and_then(|d| d.create_element("a").ok())
            .and_then(|a| a.dyn_into::<HtmlAnchorElement>().ok())
        else {
            return;
        };
        link.set_href(&url);
        link.set_download(name);
        link.click();
        if let Some(old) = with(|d| d.url.replace(url)) {
            let _ = Url::revoke_object_url(&old);
        }
        note(&format!("Saved as {name}."));
    }

    /// Import: open the box when asked, and check whatever was pasted or
    /// picked since last frame.
    pub fn check_import(sim: Res<Sim>) {
        if with(|d| std::mem::take(&mut d.import_asked)) {
            open(true);
            note(PASTE);
            tell_tests("import");
        }
        let Some(text) = with(|d| d.pasted.take()) else {
            return;
        };
        // The file and close buttons give way to "Replace your crab?" while
        // a crab is ready to load.
        set_hidden("character-confirm", true);
        set_hidden("character-import", false);
        set_text("character-summary", "");
        with(|d| d.ready = None);
        if text.trim().is_empty() {
            note(PASTE);
            tell_tests("import");
            return;
        }
        match decode(&text) {
            Ok(c) => {
                set_text("character-summary", &summary(&c, &sim));
                note("");
                set_hidden("character-confirm", false);
                set_hidden("character-import", true);
                tell_tests("ready");
                with(|d| d.ready = Some(c));
            }
            Err(why) => {
                note(why.words());
                tell_tests(&format!("refused {why:?}"));
            }
        }
    }

    /// The box's text changed: check it next frame.
    fn pasted() {
        if let Some(t) = text_box()
            && with(|d| d.importing)
        {
            let text = t.value();
            with(|d| d.pasted = Some(text));
        }
    }

    /// A file was picked: its text goes in the box, and is checked.
    fn picked() {
        let Some(input) = element::<HtmlInputElement>("character-file") else {
            return;
        };
        let Some(file) = input.files().and_then(|f| f.get(0)) else {
            return;
        };
        input.set_value("");
        if file.size() > MAX_TEXT as f64 {
            note(Refusal::TooBig.words());
            tell_tests("refused TooBig");
            return;
        }
        let reading = file.text();
        spawn_local(async move {
            let Some(text) = JsFuture::from(reading)
                .await
                .ok()
                .and_then(|v| v.as_string())
            else {
                note("Couldn't read that file.");
                return;
            };
            if let Some(t) = text_box() {
                t.set_value(&text);
            }
            with(|d| d.pasted = Some(text));
        });
    }

    /// "Replace": the save and settings replaced all together, then the page
    /// reloads into the imported crab (leaving co-op as any reload does).
    fn replace() {
        let Some(c) = with(|d| d.ready.take()) else {
            return;
        };
        let keys = writes(&c, crate::save::read_key("-settings").as_deref());
        match crate::save::replace(&keys) {
            Ok(()) => {
                info!("import: replaced the crab, reloading");
                tell_tests("replacing");
                note("Loading your crab...");
                if let Some(w) = web_sys::window() {
                    let _ = w.location().reload();
                }
            }
            Err(e) => {
                warn!("import: storage refused the character: {e}");
                note(
                    "This browser wouldn't keep it (private browsing, or storage full?). Nothing \
                     changed.",
                );
                with(|d| d.ready = Some(c));
            }
        }
    }

    fn on(id: &str, event: &str, act: impl FnMut() + 'static) {
        if let Some(e) = element::<HtmlElement>(id) {
            let f = Closure::<dyn FnMut()>::new(act);
            let _ = e.add_event_listener_with_callback(event, f.as_ref().unchecked_ref());
            f.forget();
        }
    }

    /// Give the box's buttons their jobs (once).
    fn wire() {
        if with(|d| std::mem::replace(&mut d.wired, true)) {
            return;
        }
        on("character-copy", "click", copy);
        on("character-save", "click", save_file);
        on("character-choose", "click", || {
            if let Some(input) = element::<HtmlInputElement>("character-file") {
                input.click();
            }
        });
        on("character-file", "change", picked);
        on("character-code", "input", pasted);
        on("character-replace", "click", replace);
        for id in [
            "character-keep",
            "character-close",
            "character-close-import",
        ] {
            on(id, "click", close);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crabsim::{Progress, SPAWN};

    use crate::music::Track;
    use crate::settings::Hear;

    /// A crab that has been somewhere and done things, with its own
    /// settings.
    fn travelled() -> Character {
        let mut sim = Sim::with_one_crab(Vec2::new(1400.0, 0.0));
        let mut progress = sim.fresh_progress();
        progress.climb_xp = 100.0;
        progress.fight_xp = 45.0;
        progress.lantern = 2;
        progress.eaten[0] = true;
        progress.eaten[2] = true;
        progress.larder = 3;
        progress.wakes_at = Some(1);
        sim.restore_progress(PlayerId(0), &progress);
        let settings = Settings {
            saver: true,
            hear: Hear::Nearby,
            range: 1200.0,
            master: 0.65,
            music: 0.1,
            effects: 0.9,
            voices: 1.5,
            track: Track::Calm,
        };
        Character::of(
            &sim,
            PlayerId(0),
            settings,
            ArtStyle::Ink,
            "  Big   Claw ",
            "5d3eb47, Oct 29",
            Some("pr-183".into()),
        )
        .unwrap()
    }

    #[test]
    fn a_crab_exported_and_imported_comes_back_exactly_settings_and_all() {
        let c = travelled();
        assert!(c.save.memory.is_some() && c.save.progress.is_some());
        assert_eq!(c.save.x, 1400.0);
        assert_eq!(c.name.as_deref(), Some("Big Claw"));
        let code = encode(&c);
        assert!(code.starts_with("crab:1:"), "{code}");
        assert!(
            code.chars()
                .all(|ch| ch.is_ascii_alphanumeric() || "-_:".contains(ch)),
            "text-safe: {code}"
        );
        assert_eq!(decode(&code), Ok(c.clone()));
        // A message app wraps it and adds spaces, and a file has a line break
        // at the end (and maybe a byte-order mark at the start).
        let wrapped: String = code
            .as_bytes()
            .chunks(40)
            .map(|l| String::from_utf8_lossy(l).into_owned())
            .collect::<Vec<_>>()
            .join("\n ");
        assert_eq!(decode(&format!("\u{feff}  {wrapped}\r\n")), Ok(c.clone()));
        // The live game's own crab, and a fresh one, both travel too.
        let live = Character {
            copy: None,
            build: "dev".into(),
            ..c
        };
        assert_eq!(decode(&encode(&live)), Ok(live));
        let fresh = Character::of(
            &Sim::with_one_crab(SPAWN),
            PlayerId(0),
            Settings::default(),
            ArtStyle::Oil,
            "Crab 10",
            "dev",
            None,
        )
        .unwrap();
        assert_eq!(decode(&encode(&fresh)), Ok(fresh));
    }

    #[test]
    fn importing_writes_the_save_settings_and_style_but_keeps_the_lobby_and_the_battery_saver() {
        let c = travelled();
        let mut device: std::collections::HashMap<&str, String> = [
            (".save", r#"{"version":2,"x":160.0,"y":0.0}"#.to_string()),
            ("-settings", "saver=0;master=20".to_string()),
            ("-style", "oil".to_string()),
            ("-lobby", "k3m9px".to_string()),
            ("-lobby-tab", "tab7 1700000000".to_string()),
            ("-settings-tab", "audio".to_string()),
            ("-feel", "tank".to_string()),
        ]
        .into_iter()
        .collect();
        let kept_settings = device["-settings"].clone();
        for (k, v) in writes(&c, Some(&kept_settings)) {
            device.insert(k, v);
        }
        assert_eq!(
            serde_json::from_str::<SaveData>(&device[".save"]).unwrap(),
            c.save
        );
        // Every setting comes over except the battery saver, which is this
        // device's.
        let settings = Settings::from_text(&device["-settings"]);
        assert_eq!(
            settings,
            Settings {
                saver: false,
                ..c.settings
            }
        );
        assert!(c.settings.saver);
        assert_eq!(device["-style"], "ink");
        // The name the player went by comes along, cleaned.
        assert_eq!(device[crate::names::KEPT], "Big Claw");
        // The lobby, and the rest of what's this device's, untouched.
        assert_eq!(device["-lobby"], "k3m9px");
        assert_eq!(device["-lobby-tab"], "tab7 1700000000");
        assert_eq!(device["-settings-tab"], "audio");
        assert_eq!(device["-feel"], "tank");
        // With no settings kept here, the battery saver is off.
        let fresh = writes(&c, None);
        assert!(!Settings::from_text(&fresh[1].1).saver);
        assert_eq!(
            fresh.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
            [".save", "-settings", "-style", "-name"]
        );
        // And no lobby inside the code either.
        let json = unpack(encode(&c).split(':').nth(2).unwrap());
        assert!(!json.contains("lobby"), "{json}");
    }

    /// A code's body as JSON.
    fn unpack(body: &str) -> String {
        let packed = unbase64url(body).unwrap();
        String::from_utf8(miniz_oxide::inflate::decompress_to_vec(&packed).unwrap()).unwrap()
    }

    /// A code with its body replaced by `json`, and a correct checksum.
    fn code_with(format: &str, json: &str) -> String {
        whole(
            format,
            &miniz_oxide::deflate::compress_to_vec(json.as_bytes(), 6),
        )
    }

    /// A code with its body replaced by `packed`, and a correct checksum.
    fn whole(format: &str, packed: &[u8]) -> String {
        let head = format!("crab:{format}:{}", base64url(packed));
        let sum = checksum(&head);
        format!("{head}:{sum}")
    }

    #[test]
    fn junk_damage_newer_versions_and_huge_texts_are_refused_with_plain_words() {
        use Refusal::*;
        let code = encode(&travelled());
        let refused = |text: &str| decode(text).err();
        // Junk.
        for junk in [
            "",
            "hello",
            "crab",
            "crab:",
            "crab::abc:00000000",
            "crab:0:abc:00000000",
            "crab:one:abc:00000000",
            "CRAB:1:abc:00000000",
            "{\"version\":2,\"x\":1,\"y\":2}",
        ] {
            assert_eq!(refused(junk), Some(NotACharacter), "{junk:?}");
        }
        // One letter changed, cut short anywhere, the checksum wrong.
        let mut changed = code.clone().into_bytes();
        let i = changed.len() / 2;
        changed[i] = if changed[i] == b'A' { b'B' } else { b'A' };
        assert_eq!(refused(&String::from_utf8(changed).unwrap()), Some(Damaged));
        for keep in [code.len() - 1, code.len() - 9, code.len() / 2, 9] {
            assert_eq!(refused(&code[..keep]), Some(Damaged), "cut to {keep}");
        }
        let (head, _) = code.rsplit_once(':').unwrap();
        assert_eq!(refused(&format!("{head}:00000000")), Some(Damaged));
        assert_eq!(refused(&format!("{code}:00000000")), Some(Damaged));
        // Upper-case hex in the checksum is the same checksum.
        let sum = checksum(head).to_uppercase();
        assert_eq!(decode(&format!("{head}:{sum}")), decode(&code));
        // A whole, correct code that isn't base64, isn't packed, or isn't a
        // character inside.
        assert_eq!(
            refused(&format!("crab:1:a!b:{}", checksum("crab:1:a!b"))),
            Some(NotACharacter)
        );
        assert_eq!(
            refused(&format!("crab:1:A:{}", checksum("crab:1:A"))),
            Some(NotACharacter)
        );
        assert_eq!(refused(&whole("1", b"{\"build\":1}")), Some(NotACharacter));
        assert_eq!(refused(&code_with("1", "[1, 2, 3]")), Some(NotACharacter));
        assert_eq!(
            refused(&code_with("1", r#"{"build":"x","save":{"x":1}}"#)),
            Some(NotACharacter)
        );
        assert_eq!(
            refused(&code_with(
                "1",
                r#"{"build":"x","save":{"version":2,"x":"far"}}"#
            )),
            Some(NotACharacter)
        );
        assert_eq!(
            refused(&code_with(
                "1",
                r#"{"build":"x","save":{"version":2,"x":1e300,"y":0}}"#
            )),
            Some(NotACharacter)
        );
        assert_eq!(
            refused(&code_with(
                "1",
                r#"{"build":"x","save":{"version":2,"x":5,"y":-1e300}}"#
            )),
            Some(NotACharacter)
        );
        // Newer: the code's layout, or the save inside it.
        assert_eq!(refused(&code_with("2", "anything")), Some(Newer));
        assert_eq!(refused("crab:99999999999:whatever"), Some(Newer));
        assert_eq!(
            refused(&code_with(
                "1",
                r#"{"build":"x","save":{"version":3,"x":1,"y":2,"stars":9}}"#
            )),
            Some(Newer)
        );
        // Huge: a little text unpacking into more than any crab.
        let bomb = miniz_oxide::deflate::compress_to_vec(&vec![b' '; MAX_JSON + 1], 9);
        assert!(bomb.len() * 4 / 3 < MAX_TEXT);
        assert_eq!(refused(&whole("1", &bomb)), Some(TooBig));
        let just_under = miniz_oxide::deflate::compress_to_vec(&vec![b' '; MAX_JSON], 9);
        assert_eq!(refused(&whole("1", &just_under)), Some(NotACharacter));
        // Huge: refused before looking, right at the limit.
        let padded = format!("{code}{}", " ".repeat(MAX_TEXT - code.len()));
        assert_eq!(padded.len(), MAX_TEXT);
        assert!(decode(&padded).is_ok());
        assert_eq!(refused(&format!("{padded} ")), Some(TooBig));
        // The words a player sees.
        assert_eq!(NotACharacter.words(), "That isn't a CrabScape character.");
        assert_eq!(
            Damaged.words(),
            "That character is damaged or cut short. Copy all of it and try again."
        );
        assert_eq!(
            Newer.words(),
            "That character was made by a newer version of the game. Reload the page to get it, \
             then try again."
        );
        assert_eq!(
            TooOld.words(),
            "That character is from an old version of the game it can't read."
        );
        assert_eq!(
            TooBig.words(),
            "That's far too big to be a CrabScape character."
        );
    }

    #[test]
    fn a_crab_saved_by_an_older_game_imports_and_one_too_old_to_read_is_refused() {
        // Saved before skills, moulting, the workbench and settings' music:
        // what it lacks starts fresh, as with a save kept on the device.
        let old = code_with(
            "1",
            r#"{"build":"1a2b3c4, Aug 2","save":{"version":2,"x":900.0,"y":0.0,
               "progress":{"upgrades":2,"lantern":1,"scissor_claw":true,"climb_xp":40.0,
               "carrying":null,"eaten":[true,false],"cultures_taken":[true],"clues_found":[],
               "teeth_taken":[],"nets_cut":[],"cast_offs":[]}},
               "settings":"saver=0;hear=everyone;range=900;game=65;voices=150"}"#,
        );
        let c = decode(&old).unwrap();
        assert_eq!((c.save.x, c.save.y), (900.0, 0.0));
        let p = c.save.progress.as_ref().unwrap();
        assert_eq!((p.upgrades, p.lantern, p.climb_xp), (2, 1, 40.0));
        assert_eq!((p.swim_xp, p.moults, p.larder), (0.0, 0, 0));
        assert!(p.feats.is_empty() && p.scissor_claw);
        assert_eq!(c.settings.effects, 0.65);
        assert_eq!(c.style, ArtStyle::default());
        // From before names: none, and importing it leaves the name here be.
        assert_eq!(c.name, None);
        assert!(
            writes(&c, None)
                .iter()
                .all(|(k, _)| *k != crate::names::KEPT)
        );
        // It loads into this level like any save.
        let mut sim = Sim::with_one_crab(Vec2::new(c.save.x, c.save.y));
        sim.restore_progress(PlayerId(0), p);
        assert_eq!(sim.crab(PlayerId(0)).unwrap().mover.upgrades, 2);
        // Version 1 saves (before positions) the game never read.
        assert_eq!(
            decode(&code_with(
                "1",
                r#"{"build":"x","save":{"version":1,"tile":{"x":3,"y":4}}}"#
            )),
            Err(Refusal::TooOld)
        );
    }

    #[test]
    fn the_importer_says_what_it_is_about_to_load() {
        let c = travelled();
        let sim = Sim::with_one_crab(SPAWN);
        let eggs = sim.level.eggs.len();
        assert_eq!(
            summary(&c, &sim),
            format!(
                "Climbing 3, Swimming 1, Fighting 2\n2 of {eggs} eggs eaten, 2 cultures in its \
                 lantern\n3 m from home\nFrom preview pr-183, build 5d3eb47, Oct 29"
            )
        );
        let home = sim.level.home_den;
        let mut at_home = Character {
            copy: Some("branches-big-idea".into()),
            ..c.clone()
        };
        (at_home.save.x, at_home.save.y) = (home.x + 400.0, home.y);
        at_home.save.progress.as_mut().unwrap().lantern = 1;
        let lines = summary(&at_home, &sim);
        let lines: Vec<_> = lines.lines().collect();
        assert_eq!(
            lines[1],
            format!("2 of {eggs} eggs eaten, 1 culture in its lantern")
        );
        assert_eq!(lines[2], "At home");
        assert_eq!(lines[3], "From branch big-idea, build 5d3eb47, Oct 29");
        (at_home.save.x, at_home.save.y) = (home.x + 480.0, home.y);
        at_home.save.progress = None;
        at_home.copy = None;
        let lines = summary(&at_home, &sim);
        let lines: Vec<_> = lines.lines().collect();
        assert_eq!(lines[0], "Climbing 1, Swimming 1, Fighting 1");
        assert_eq!(
            lines[1],
            format!("0 of {eggs} eggs eaten, 0 cultures in its lantern")
        );
        assert_eq!(lines[2], "1 m from home");
        assert_eq!(lines[3], "From the live game, build 5d3eb47, Oct 29");
    }

    #[test]
    fn the_file_is_named_for_the_day_and_the_preview() {
        assert_eq!(file_name((2026, 10, 9), None), "crabscape-2026-10-09.crab");
        assert_eq!(
            file_name((2026, 1, 29), Some("pr-183")),
            "crabscape-pr-183-2026-01-29.crab"
        );
    }

    #[test]
    fn base64url_round_trips_and_refuses_what_it_never_writes() {
        assert_eq!(base64url(&[0xfb, 0xef, 0xbe]), "----");
        assert_eq!(base64url(&[0xff, 0xff, 0xff]), "____");
        assert_eq!(base64url(b"crab"), "Y3JhYg");
        assert_eq!(base64url(b"AZaz09"), "QVphejA5");
        for bytes in [&b""[..], b"c", b"cr", b"cra", b"crab", b"crabs"] {
            assert_eq!(unbase64url(&base64url(bytes)).as_deref(), Some(bytes));
        }
        assert_eq!(unbase64url("Y3JhYg").unwrap(), b"crab");
        assert_eq!(unbase64url("Y3JhY"), None);
        assert_eq!(unbase64url("Y3J+Yg"), None);
        assert_eq!(unbase64url("Y3J=Yg"), None);
    }

    #[test]
    fn progress_survives_the_trip_into_a_new_game() {
        let c = travelled();
        let back = decode(&encode(&c)).unwrap();
        let mut sim = Sim::with_one_crab(Vec2::new(back.save.x, back.save.y));
        sim.restore_progress(PlayerId(0), back.save.progress.as_ref().unwrap());
        let p: Progress = sim.progress(PlayerId(0)).unwrap();
        assert_eq!(
            p.ignoring_clocks(),
            c.save.progress.as_ref().unwrap().ignoring_clocks()
        );
    }
}

/// Whatever anyone pastes or picks, the importer answers without crashing.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod properties {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn any_text_is_answered_never_a_crash(text in any::<String>()) {
            let _ = decode(&text);
        }

        #[test]
        fn any_text_that_looks_like_a_code_is_answered(
            body in "[A-Za-z0-9_-]{0,200}",
            format in 0u32..4,
            good_sum in any::<bool>(),
        ) {
            let head = format!("crab:{format}:{body}");
            let sum = if good_sum { checksum(&head) } else { "0badc0de".into() };
            let _ = decode(&format!("{head}:{sum}"));
        }

        #[test]
        fn any_json_inside_a_whole_code_is_answered(json in any::<String>(), packed in any::<bool>()) {
            let body = if packed {
                miniz_oxide::deflate::compress_to_vec(json.as_bytes(), 6)
            } else {
                json.into_bytes()
            };
            let head = format!("crab:1:{}", base64url(&body));
            let sum = checksum(&head);
            let _ = decode(&format!("{head}:{sum}"));
        }

        #[test]
        fn any_bytes_round_trip_through_base64url(bytes in proptest::collection::vec(any::<u8>(), 0..300)) {
            prop_assert_eq!(unbase64url(&base64url(&bytes)), Some(bytes));
        }
    }
}
