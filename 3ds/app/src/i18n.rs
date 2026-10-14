// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::OnceLock;

use ctru::services::cfgu::Cfgu;
use ctru::services::cfgu::Language;
static SPANISH: AtomicBool = AtomicBool::new(false);
static EN: OnceLock<HashMap<String, String>> = OnceLock::new();
static ES: OnceLock<HashMap<String, String>> = OnceLock::new();
const PATH: &str = "sdmc:/3ds/anki-3ds/language.txt";

pub fn init() {
    let system_spanish =
        Cfgu::new().ok().and_then(|c| c.language().ok()) == Some(Language::Spanish);
    let spanish = match std::fs::read_to_string(PATH).ok().as_deref().map(str::trim) {
        Some("es") => true,
        Some("en") => false,
        _ => system_spanish,
    };
    SPANISH.store(spanish, Ordering::Relaxed);
    EN.get_or_init(|| {
        serde_json::from_str(include_str!("../locales/en.json")).expect("English catalog")
    });
    ES.get_or_init(|| {
        serde_json::from_str(include_str!("../locales/es.json")).expect("Spanish catalog")
    });
}
pub fn language() -> &'static str {
    if SPANISH.load(Ordering::Relaxed) {
        "es"
    } else {
        "en"
    }
}
pub fn toggle() -> std::io::Result<()> {
    let spanish = !SPANISH.load(Ordering::Relaxed);
    std::fs::write(PATH, if spanish { "es" } else { "en" })?;
    SPANISH.store(spanish, Ordering::Relaxed);
    Ok(())
}
pub fn tr(message: &'static str) -> &'static str {
    let catalog = if SPANISH.load(Ordering::Relaxed) {
        ES.get()
    } else {
        EN.get()
    };
    catalog
        .and_then(|c| c.get(message))
        .map(String::as_str)
        .unwrap_or(message)
}
pub fn format(message: &'static str, values: &[&str]) -> String {
    let mut text = tr(message).to_owned();
    for (index, value) in values.iter().enumerate() {
        text = text.replace(&format!("{{{index}}}"), value);
    }
    text
}
