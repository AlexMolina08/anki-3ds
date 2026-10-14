// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::path::Path;

use anki::collection::CollectionBuilder;
use anki::prelude::*;
use anki::services::SchedulerService;
use anki_proto::scheduler::card_answer::Rating;
use anki_proto::scheduler::GradeNowRequest;

use super::harness::ProbeResult;
use super::harness::CASES;

pub fn prepare(root: &Path) -> ProbeResult<()> {
    if root.exists() {
        return Err(format!("refusing to overwrite input fixtures: {}", root.display()).into());
    }
    std::fs::create_dir_all(root)?;
    let base = root.join("base.anki2");
    let mut col = CollectionBuilder::new(&base).build()?;
    col.set_configured_utc_offset(0)?;
    col.set_config_json("creationOffset", &0, false)?;
    col.set_config_bool(BoolKey::Fsrs, true, false)?;
    let nt = col.get_notetype_by_name("Basic")?.unwrap();
    for label in ["seed-one", "seed-two"] {
        let mut note = nt.new_note();
        note.set_field(0, label)?;
        note.set_field(1, "disposable")?;
        col.add_note(&mut note, DeckId(1))?;
    }
    let cid: i64 =
        col.storage
            .db()
            .query_row("select id from cards order by id limit 1", [], |r| r.get(0))?;
    let _ = SchedulerService::grade_now(
        &mut col,
        GradeNowRequest {
            card_ids: vec![cid],
            rating: Rating::Good as i32,
        },
    )?;
    for table in [
        "notes",
        "cards",
        "revlog",
        "notetypes",
        "decks",
        "deck_config",
        "tags",
        "config",
    ] {
        col.storage
            .db()
            .execute(&format!("update {table} set usn=0"), [])?;
    }
    let stamp = TimestampMillis::now().0 - 10_000;
    col.storage
        .db()
        .execute("update col set usn=1, mod=?1, scm=?1, ls=?1", [stamp])?;
    col.close(None)?;
    for &case in CASES {
        let dir = root.join(case);
        std::fs::create_dir(&dir)?;
        for side in ["client", "server"] {
            let path = dir.join(format!("{side}.anki2"));
            std::fs::copy(&base, &path)?;
            let mut col = CollectionBuilder::new(&path)
                .set_server(side == "server")
                .build()?;
            let nid: i64 = col.storage.db().query_row(
                "select id from notes order by id limit 1",
                [],
                |r| r.get(0),
            )?;
            let second: i64 = col.storage.db().query_row(
                "select id from notes order by id desc limit 1",
                [],
                |r| r.get(0),
            )?;
            let changes = match case {
                "local" | "interruption" | "cancellation" => side == "client",
                "remote" => side == "server",
                "both" | "conflict" | "review" | "deletion" => true,
                _ => false,
            };
            if changes {
                match case {
                    "review" => {
                        let cid: i64 = col.storage.db().query_row(
                            "select id from cards order by id desc limit 1",
                            [],
                            |r| r.get(0),
                        )?;
                        if side == "client" {
                            let _ = SchedulerService::grade_now(
                                &mut col,
                                GradeNowRequest {
                                    card_ids: vec![cid],
                                    rating: Rating::Easy as i32,
                                },
                            )?;
                        } else {
                            let cid: i64 = col.storage.db().query_row(
                                "select id from cards order by id limit 1",
                                [],
                                |r| r.get(0),
                            )?;
                            let _ = SchedulerService::grade_now(
                                &mut col,
                                GradeNowRequest {
                                    card_ids: vec![cid],
                                    rating: Rating::Hard as i32,
                                },
                            )?;
                        }
                    }
                    "deletion" => {
                        col.remove_notes(&[NoteId(if side == "client" { nid } else { second })])?;
                    }
                    _ => {
                        let id = if case == "both" && side == "server" {
                            second
                        } else {
                            nid
                        };
                        let mut note = col.storage.get_note(NoteId(id))?.unwrap();
                        note.set_field(0, format!("{case}-{side}"))?;
                        col.update_note(&mut note)?;
                        col.storage.db().execute(
                            "update notes set mod=?1 where id=?2",
                            [stamp / 1000 + if side == "client" { 2 } else { 3 }, id],
                        )?;
                    }
                }
                col.storage.db().execute(
                    "update col set mod=?1",
                    [stamp + if side == "client" { 2_000 } else { 3_000 }],
                )?;
            }
            if case == "review" {
                // Distinct input mtimes avoid an accidental same-second tie
                // between competing daily deck statistics.
                col.storage.db().execute(
                    "update decks set mtime_secs=?1",
                    [stamp / 1000 + if side == "client" { 2 } else { 3 }],
                )?;
            }
            if case == "full_required" && side == "server" {
                col.storage
                    .db()
                    .execute("update col set scm=?1, mod=?1", [stamp + 4_000])?;
            }
            col.close(None)?;
        }
    }
    Ok(())
}
