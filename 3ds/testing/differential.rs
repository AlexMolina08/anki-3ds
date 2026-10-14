// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use serde_json::json;
use serde_json::Value;

use super::*;

pub(crate) fn run() -> Value {
    let mut scenarios = Vec::new();
    for fsrs_enabled in [false, true] {
        for production_fuzz in [false, true] {
            std::env::set_var("ANKI_3DS_FIXED_TIME", "1700049600000");
            let root = std::env::var("ANKI_3DS_DIFFERENTIAL_ROOT")
                .unwrap_or_else(|_| "sdmc:/3ds/anki-3ds".into());
            let path = format!(
                "{root}/differential-{}-{}.anki2",
                u8::from(fsrs_enabled),
                u8::from(production_fuzz)
            );
            if std::path::Path::new(&path).exists() {
                std::fs::remove_file(&path).unwrap();
            }
            let mut col = crate::collection::CollectionBuilder::new(&path)
                .build()
                .unwrap();
            col.set_configured_utc_offset(0).unwrap();
            col.set_creation_utc_offset(Some(0)).unwrap();
            col.set_config_bool(BoolKey::Fsrs, fsrs_enabled, false)
                .unwrap();
            let mut note = crate::tests::NoteAdder::basic(&mut col)
                .fields(&["controlled front", "controlled back"])
                .add(&mut col);
            note.guid = "fixed-port-guid".into();
            col.storage.update_note(&note).unwrap();
            let cid = col.get_first_card().id;
            let mut history = Vec::new();
            for (step, (seconds, rating)) in [
                (0, Rating::Again),
                (60, Rating::Good),
                (660, Rating::Good),
                (3 * 86400, Rating::Again),
                (3 * 86400 + 600, Rating::Good),
                (4 * 86400, Rating::Hard),
                (8 * 86400, Rating::Easy),
            ]
            .into_iter()
            .enumerate()
            {
                let millis = 1700049600000_i64 + i64::from(seconds) * 1000;
                std::env::set_var("ANKI_3DS_FIXED_TIME", millis.to_string());
                col.state.scheduler_info = None;
                col.clear_study_queues();
                let card = col.storage.get_card(cid).unwrap().unwrap();
                let mut updater = col.card_state_updater(card).unwrap();
                if production_fuzz {
                    updater.fuzz_seed = Some((cid.0 as u64).wrapping_add(updater.card.reps as u64));
                }
                let current = updater.current_card_state();
                let states = current.next_states(&updater.state_context(None));
                let next = match rating {
                    Rating::Again => states.again,
                    Rating::Hard => states.hard,
                    Rating::Good => states.good,
                    Rating::Easy => states.easy,
                };
                let partial = updater.apply_study_state(current, next).unwrap();
                let button = match rating {
                    Rating::Again => 1,
                    Rating::Hard => 2,
                    Rating::Good => 3,
                    Rating::Easy => 4,
                };
                let revlog =
                    partial.into_revlog_entry(Usn(-1), cid, button, TimestampMillis(millis), 1234);
                col.storage
                    .add_revlog_entry(&revlog, false)
                    .unwrap()
                    .unwrap();
                let mut updated = updater.into_card();
                updated.last_review_time = Some(TimestampSecs(millis / 1000));
                col.storage.update_card(&updated).unwrap();
                let stored = col.storage.get_card(cid).unwrap().unwrap();
                assert_eq!(
                    (
                        stored.ctype,
                        stored.queue,
                        stored.due,
                        stored.interval,
                        stored.ease_factor,
                        stored.reps,
                        stored.lapses,
                        stored.remaining_steps
                    ),
                    (
                        updated.ctype,
                        updated.queue,
                        updated.due,
                        updated.interval,
                        updated.ease_factor,
                        updated.reps,
                        updated.lapses,
                        updated.remaining_steps
                    )
                );
                assert_eq!(
                    stored.memory_state.is_some(),
                    updated.memory_state.is_some()
                );
                history.push(json!({"step":step,"card":format!("{stored:?}"),
                    "computed_memory": updated.memory_state.map(|m|json!({"stability":m.stability,"difficulty":m.difficulty})),
                    "memory": stored.memory_state.map(|m|json!({"stability":m.stability,"difficulty":m.difficulty})),
                    "due":stored.due,"interval":stored.interval,"ease":stored.ease_factor,
                    "type":stored.ctype as u8,"queue":stored.queue as i8,
                    "reps":stored.reps,"lapses":stored.lapses,"remaining_steps":stored.remaining_steps,
                    "revlog":revlog}));
            }
            let rows = col.storage.get_all_revlog_entries_in_card_order().unwrap();
            assert_eq!(rows.len(), 7);
            assert_eq!(col.storage.get_note(note.id).unwrap().unwrap().id, note.id);
            scenarios.push(
                json!({"fsrs":fsrs_enabled,"production_fuzz_seed":production_fuzz,
                "history":history,"stored_revlog":rows}),
            );
            let last = col.get_first_card();
            col.close(None).unwrap();
            let reopened = crate::collection::CollectionBuilder::new(&path)
                .build()
                .unwrap();
            assert_eq!(reopened.get_first_card(), last);
            assert_eq!(
                reopened
                    .storage
                    .get_all_revlog_entries_in_card_order()
                    .unwrap(),
                rows
            );
            scenarios.last_mut().unwrap()["disk_reopened"] = json!(true);
            reopened.close(None).unwrap();
        }
    }
    std::env::remove_var("ANKI_3DS_FIXED_TIME");
    json!({"scenarios":scenarios})
}
