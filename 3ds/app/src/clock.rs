// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use super::i18n::tr;
use super::Ui;
const PATH: &str = "sdmc:/3ds/anki-3ds/utc-offset-minutes.txt";
extern "C" {
    fn anki_set_utc_offset(minutes: i32);
}

pub fn configure(force: bool) -> Result<i32, String> {
    let saved = std::fs::read_to_string(PATH).ok();
    let initial = saved.as_deref().unwrap_or("0").trim();
    let value = if !force && saved.is_some() {
        initial.to_owned()
    } else {
        Ui::keyboard(
            tr("UTC offset in minutes (Madrid: 120 summer, 60 winter)"),
            initial,
        )?
        .ok_or(tr("Console time zone required"))?
    };
    let minutes: i32 = value.trim().parse().map_err(|_| tr("Invalid UTC offset"))?;
    if !(-840..=840).contains(&minutes) {
        return Err(tr("UTC offset out of range").into());
    }
    unsafe {
        anki_set_utc_offset(minutes);
    }
    std::fs::write(PATH, format!("{minutes}\n")).map_err(|e| e.to_string())?;
    Ok(minutes)
}

pub fn offset() -> i32 {
    std::fs::read_to_string(PATH)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0)
}
