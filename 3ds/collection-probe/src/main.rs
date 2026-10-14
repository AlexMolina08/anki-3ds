// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html


use anki::collection::CollectionBuilder;
use anki::services::CardsService;
use anki::services::ConfigService;
use anki::services::SchedulerService;
use anki::timestamp::TimestampMillis;
use anki_proto::cards::CardId;
use anki_proto::config::SetConfigJsonRequest;
use anki_proto::scheduler::card_answer::Rating;
use anki_proto::scheduler::CardAnswer;
use anki_proto::scheduler::GetQueuedCardsRequest;
use anki_proto::scheduler::GradeNowRequest;
use ctru::prelude::*;

const ROOT: &str = "sdmc:/3ds/anki-3ds";
const COLLECTION: &str = "sdmc:/3ds/anki-3ds/disposable.anki2";
const RESULT: &str = "sdmc:/3ds/anki-3ds/collection-result.txt";
const FSRS_MARKER: &str = "sdmc:/3ds/anki-3ds/fsrs-mode";

const CARD_COLUMNS: [&str; 18] = [
    "id", "nid", "did", "ord", "mod", "usn", "type", "queue", "due", "ivl", "factor", "reps",
    "lapses", "left", "odue", "odid", "flags", "data",
];

/// Same for `revlog`, which is what actually proves a review was recorded.
const REVLOG_COLUMNS: [&str; 9] = [
    "id", "cid", "usn", "ease", "ivl", "lastIvl", "factor", "time", "type",
];

type Probe = Result<String, Box<dyn std::error::Error>>;

/// AnkiError's own Display is just a category, so the SQLite cause underneath
/// has to be walked explicitly or every storage failure looks identical.
fn describe(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(&format!(": {cause}"));
        source = cause.source();
    }
    text
}

/// Open the collection directly, outside Anki's storage layer.
///
/// The VFS has no shared memory, so a read-only connection cannot run WAL
/// recovery and SQLite rejects it. Exclusive locking on a read-write handle
/// keeps the reader from touching a collection another client owns.
fn open_raw() -> Result<rusqlite::Connection, Box<dyn std::error::Error>> {
    let connection = rusqlite::Connection::open_with_flags_and_vfs(
        COLLECTION,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
        "anki-3ds-sd",
    )?;
    connection.pragma_update(None, "locking_mode", "exclusive")?;
    Ok(connection)
}

fn rows(
    connection: &rusqlite::Connection,
    table: &str,
    columns: &[&str],
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let sql = format!("select {} from {table} order by id", columns.join(", "));
    let mut statement = connection.prepare(&sql)?;
    let mut out = Vec::new();
    let mut query = statement.query([])?;
    while let Some(row) = query.next()? {
        let mut cells = Vec::with_capacity(columns.len());
        for index in 0..columns.len() {
            let value: rusqlite::types::Value = row.get(index)?;
            cells.push(match value {
                rusqlite::types::Value::Null => "NULL".to_string(),
                rusqlite::types::Value::Integer(number) => number.to_string(),
                rusqlite::types::Value::Real(number) => format!("{number}"),
                rusqlite::types::Value::Text(text) => text,
                rusqlite::types::Value::Blob(blob) => format!("<{} bytes>", blob.len()),
            });
        }
        out.push(cells.join("|"));
    }
    Ok(out)
}

enum Offered {
    /// Seconds until the card is due again, from the learning state.
    LearningSecs(u32),
    /// Whole days until the card is due again, from the review state.
    ReviewDays(u32),
}

fn offered(
    state: Option<&anki_proto::scheduler::SchedulingState>,
) -> Result<Offered, Box<dyn std::error::Error>> {
    use anki_proto::scheduler::scheduling_state::normal::Kind;
    use anki_proto::scheduler::scheduling_state::Kind as Outer;
    let outer = state
        .and_then(|state| state.kind.as_ref())
        .ok_or("the button offered no state")?;
    let normal = match outer {
        Outer::Normal(normal) => normal,
        Outer::Filtered(_) => return Err("a filtered deck state is not expected here".into()),
    };
    match normal.kind.as_ref() {
        Some(Kind::Learning(learning)) => Ok(Offered::LearningSecs(learning.scheduled_secs)),
        Some(Kind::Relearning(relearning)) => Ok(Offered::LearningSecs(
            relearning
                .learning
                .as_ref()
                .ok_or("relearning state missing its learning half")?
                .scheduled_secs,
        )),
        Some(Kind::Review(review)) => Ok(Offered::ReviewDays(review.scheduled_days)),
        _ => Err("the button offered a state this probe does not schedule".into()),
    }
}

fn state_kind(state: &anki_proto::scheduler::SchedulingState) -> &'static str {
    use anki_proto::scheduler::scheduling_state::normal::Kind;
    use anki_proto::scheduler::scheduling_state::Kind as Outer;
    match state.kind.as_ref() {
        Some(Outer::Normal(normal)) => match normal.kind.as_ref() {
            Some(Kind::New(_)) => "new",
            Some(Kind::Learning(_)) => "learning",
            Some(Kind::Review(_)) => "review",
            Some(Kind::Relearning(_)) => "relearning",
            None => "normal/empty",
        },
        Some(Outer::Filtered(_)) => "filtered",
        None => "empty",
    }
}

fn upstream_card(
    collection: &mut anki::collection::Collection,
    id: i64,
) -> Result<anki_proto::cards::Card, Box<dyn std::error::Error>> {
    Ok(CardsService::get_card(
        collection,
        anki_proto::cards::CardId { cid: id },
    )?)
}

/// The FSRS memory state, as upstream exposes it on the card itself.
struct MemoryState {
    stability: f64,
    difficulty: f64,
    desired_retention: f64,
}

fn fsrs_memory_state(
    card: &anki_proto::cards::Card,
) -> Result<Option<MemoryState>, Box<dyn std::error::Error>> {
    let Some(state) = card.memory_state.as_ref() else {
        return Ok(None);
    };
    Ok(Some(MemoryState {
        stability: state.stability as f64,
        difficulty: state.difficulty as f64,
        desired_retention: card
            .desired_retention
            .ok_or("card has a memory state but no desired retention")?
            as f64,
    }))
}

fn expected_ease(
    card: &anki_proto::cards::Card,
    fsrs: bool,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let Some(state) = fsrs_memory_state(card)? else {
        if fsrs {
            return Err("FSRS was enabled but the card has no memory state".into());
        }
        return Ok(None);
    };
    if !fsrs {
        return Err("a memory state was written without FSRS being enabled".into());
    }
    if state.stability <= 0.0 || !(1.0..=10.0).contains(&state.difficulty) {
        return Err(format!(
            "implausible FSRS memory state: stability={} difficulty={}",
            state.stability, state.difficulty
        )
        .into());
    }
    if state.desired_retention <= 0.0 || state.desired_retention > 1.0 {
        return Err(format!("implausible desired retention: {}", state.desired_retention).into());
    }
    Ok(Some(format!(
        "{}",
        (((state.difficulty - 1.0) / 9.0 + 0.1) * 1000.0).round() as u32
    )))
}

fn persisted_memory_state(
    card_row: &str,
) -> Result<Option<serde_json::Value>, Box<dyn std::error::Error>> {
    let data = card_row
        .rsplit('|')
        .next()
        .ok_or("card row has no data column")?;
    let parsed: serde_json::Value = serde_json::from_str(data)?;
    let Some(object) = parsed.as_object() else {
        return Err(format!("card data is not a JSON object: {data}").into());
    };
    if !object.contains_key("s") {
        return Ok(None);
    }
    Ok(Some(parsed))
}

fn verify_persisted(card_id: i64, expected_card: &str, expected_revlog: &[String]) -> Probe {
    let connection = open_raw()?;
    let raw_card = rows(&connection, "cards", &CARD_COLUMNS)?;
    let raw_revlog = rows(&connection, "revlog", &REVLOG_COLUMNS)?;
    drop(connection);
    if raw_card.len() != 1 {
        return Err(format!("expected 1 card row after reopen, found {}", raw_card.len()).into());
    }
    if raw_card[0] != expected_card {
        return Err(format!(
            "card row changed across reopen\n  before {expected_card}\n  after  {}",
            raw_card[0]
        )
        .into());
    }
    if raw_revlog != expected_revlog {
        return Err(format!("revlog rows changed across reopen: {raw_revlog:?}").into());
    }

    let mut collection = CollectionBuilder::new(COLLECTION).build()?;
    let card = upstream_card(&mut collection, card_id)?;
    let through_upstream = format!(
        "type={} queue={} due={} interval={} reps={} lapses={} remaining_steps={}",
        card.ctype,
        card.queue,
        card.due,
        card.interval,
        card.reps,
        card.lapses,
        card.remaining_steps
    );
    collection.close(None)?;
    Ok(through_upstream)
}

fn review() -> Probe {
    std::fs::metadata(COLLECTION)?;
    anki_3ds_sqlite_vfs::register().map_err(|code| format!("VFS registration: {code}"))?;

    let mut collection = CollectionBuilder::new(COLLECTION).build()?;

    let fsrs = std::fs::metadata(FSRS_MARKER).is_ok();
    if fsrs {
        let changes = ConfigService::set_config_json(
            &mut collection,
            SetConfigJsonRequest {
                key: "fsrs".to_string(),
                value_json: b"true".to_vec(),
                undoable: false,
            },
        )?;
        if !changes.config {
            return Err(format!("enabling FSRS reported unexpected changes: {changes:?}").into());
        }
        if !collection.fsrs_enabled() {
            return Err("FSRS was requested but the backend reports it disabled".into());
        }
    }

    // A review must be logged with the reviewer's own clock, so read it from
    // the same source the backend uses instead of a wall-clock constant. The
    // call also proves the reviewed clock runs on the console.
    let first_at = TimestampMillis::now().0;
    if first_at <= 0 {
        return Err(format!("Horizon clock reported {first_at}ms").into());
    }
    let timing = SchedulerService::sched_timing_today(&mut collection)?;

    let queued = SchedulerService::get_queued_cards(
        &mut collection,
        GetQueuedCardsRequest {
            fetch_limit: 1,
            intraday_learning_only: false,
        },
    )?;
    if queued.new_count != 1 || queued.learning_count != 0 || queued.review_count != 0 {
        return Err(format!(
            "fixture should start with exactly one new card, got new={} learning={} review={}",
            queued.new_count, queued.learning_count, queued.review_count
        )
        .into());
    }
    let queued_card = queued.cards.first().ok_or("no card was queued")?;
    let card_id = queued_card
        .card
        .as_ref()
        .ok_or("queued card missing card data")?
        .id;
    let states = queued_card
        .states
        .as_ref()
        .ok_or("queued card missing states")?;
    let new_card = upstream_card(&mut collection, card_id)?;
    if new_card.ctype != 0 || new_card.queue != 0 {
        return Err(format!(
            "fixture card should be new, got type={} queue={}",
            new_card.ctype, new_card.queue
        )
        .into());
    }

    let first_offered = offered(states.good.as_ref())?;
    let changes = SchedulerService::answer_card(
        &mut collection,
        CardAnswer {
            card_id,
            current_state: states.current.clone(),
            new_state: states.good.clone(),
            rating: Rating::Good as i32,
            answered_at_millis: first_at,
            milliseconds_taken: 1500,
        },
    )?;
    if !changes.card || !changes.study_queues {
        return Err(format!("answer_card reported unexpected changes: {changes:?}").into());
    }
    let learning_card = upstream_card(&mut collection, card_id)?;
    let Offered::LearningSecs(step) = first_offered else {
        return Err("a new card answered Good should enter learning".into());
    };
    if learning_card.reps != 1 {
        return Err(format!("expected one review, got reps={}", learning_card.reps).into());
    }
    if learning_card.queue != 1 || learning_card.ctype != 1 {
        return Err(format!(
            "Good should leave a new card in the learning queue, got queue={} type={}",
            learning_card.queue, learning_card.ctype
        )
        .into());
    }
    if learning_card.interval != 0 || learning_card.remaining_steps != 1 {
        return Err(format!(
            "expected no day interval and one step remaining, got interval={} remaining_steps={}",
            learning_card.interval, learning_card.remaining_steps
        )
        .into());
    }
    let first_at_secs = first_at / 1000;
    let fuzz = ((step as f32) * 0.25).min(300.0).floor() as i64;
    let earliest = first_at_secs + step as i64;
    let latest = earliest + fuzz;
    if (learning_card.due as i64) < earliest || (learning_card.due as i64) > latest {
        return Err(format!(
            "due {} is outside the expected {earliest}..={latest} window for a {step}s step plus fuzz",
            learning_card.due
        )
        .into());
    }
    let observed_fuzz = learning_card.due as i64 - earliest;

    // ---- Second answer: the card graduates to a review card. --------------
    //
    // A card in learning is offered even when its step has not elapsed, so the
    // step does not have to be waited out; the scheduler graduates the card on
    // the answer alone. The answer is stamped with the real clock rather than a
    // time advanced past the step, because the revlog is keyed by that stamp:
    // a fabricated future time would write this row after the lapse below and
    // leave the log out of order, which no real client can produce.
    let second_at = TimestampMillis::now().0;
    let queued = SchedulerService::get_queued_cards(
        &mut collection,
        GetQueuedCardsRequest {
            fetch_limit: 1,
            intraday_learning_only: false,
        },
    )?;
    if queued.new_count != 0 || queued.learning_count != 1 || queued.review_count != 0 {
        return Err(format!(
            "the learning card should be the only one queued, got new={} learning={} review={}",
            queued.new_count, queued.learning_count, queued.review_count
        )
        .into());
    }
    let queued_card = queued
        .cards
        .first()
        .ok_or("the learning card was not queued")?;
    let states = queued_card
        .states
        .as_ref()
        .ok_or("queued card missing states")?;
    let second_offered = offered(states.good.as_ref())?;
    let changes = SchedulerService::answer_card(
        &mut collection,
        CardAnswer {
            card_id,
            current_state: states.current.clone(),
            new_state: states.good.clone(),
            rating: Rating::Good as i32,
            answered_at_millis: second_at,
            milliseconds_taken: 2500,
        },
    )?;
    if !changes.card || !changes.study_queues {
        return Err(
            format!("the graduating answer reported unexpected changes: {changes:?}").into(),
        );
    }
    let graduated_card = upstream_card(&mut collection, card_id)?;
    let Offered::ReviewDays(days) = second_offered else {
        return Err(format!(
            "a card that has finished its learning steps should graduate, but Good offered {}",
            state_kind(states.good.as_ref().ok_or("no good state")?)
        )
        .into());
    };
    if graduated_card.ctype != 2 || graduated_card.queue != 2 {
        return Err(format!(
            "a graduated card should be a review card, got queue={} type={}",
            graduated_card.queue, graduated_card.ctype
        )
        .into());
    }
    if graduated_card.reps != 2 || graduated_card.remaining_steps != 0 {
        return Err(format!(
            "expected two reviews and no steps left, got reps={} remaining_steps={}",
            graduated_card.reps, graduated_card.remaining_steps
        )
        .into());
    }
    let expected_due = timing.days_elapsed as i32 + days as i32;
    if graduated_card.interval != days || graduated_card.due != expected_due {
        return Err(format!(
            "expected interval {days} and due {expected_due} (day {} + {days}), got interval={} due={}",
            timing.days_elapsed, graduated_card.interval, graduated_card.due
        )
        .into());
    }

    let states = SchedulerService::get_scheduling_states(&mut collection, CardId { cid: card_id })?;
    let current = states
        .current
        .as_ref()
        .ok_or("the review card has no current state")?;
    if state_kind(current) != "review" {
        return Err(format!(
            "expected a review card before the lapse, got {}",
            state_kind(current)
        )
        .into());
    }
    let again = states.again.as_ref().ok_or("no Again state was offered")?;
    let (relearn_secs, relearn_steps, lapse_interval_days) = match offered(Some(again))? {
        Offered::LearningSecs(secs) => {
            use anki_proto::scheduler::scheduling_state::normal::Kind;
            use anki_proto::scheduler::scheduling_state::Kind as Outer;
            let Some(Outer::Normal(normal)) = again.kind.as_ref() else {
                return Err("Again did not offer a normal state".into());
            };
            let Some(Kind::Relearning(relearning)) = normal.kind.as_ref() else {
                return Err(
                    format!("a lapse should offer relearning, got {}", state_kind(again)).into(),
                );
            };
            let learning = relearning
                .learning
                .as_ref()
                .ok_or("relearning state missing its learning half")?;
            let review = relearning
                .review
                .as_ref()
                .ok_or("relearning state missing its review half")?;
            (secs, learning.remaining_steps, review.scheduled_days)
        }
        Offered::ReviewDays(_) => {
            return Err("a lapse should offer a relearning step, not a day interval".into())
        }
    };
    if relearn_secs != 600 {
        return Err(format!("the default relearn step should be 600s, got {relearn_secs}").into());
    }

    let lapse_started_at = TimestampMillis::now().0;
    let changes = SchedulerService::grade_now(
        &mut collection,
        GradeNowRequest {
            card_ids: vec![card_id],
            rating: Rating::Again as i32,
        },
    )?;
    let lapse_finished_at = TimestampMillis::now().0;
    if !changes.card || !changes.study_queues {
        return Err(format!("grade_now reported unexpected changes: {changes:?}").into());
    }
    let lapsed_card = upstream_card(&mut collection, card_id)?;
    if lapsed_card.ctype != 3 || lapsed_card.queue != 1 {
        return Err(format!(
            "a lapsed card should be relearning in the learning queue, got type={} queue={}",
            lapsed_card.ctype, lapsed_card.queue
        )
        .into());
    }
    if lapsed_card.lapses != 1 {
        return Err(format!(
            "the first lapse should be counted, got lapses={}",
            lapsed_card.lapses
        )
        .into());
    }
    if lapsed_card.reps != 3 || lapsed_card.remaining_steps != relearn_steps {
        return Err(format!(
            "expected three reviews and {relearn_steps} steps left, got reps={} remaining_steps={}",
            lapsed_card.reps, lapsed_card.remaining_steps
        )
        .into());
    }
    if lapsed_card.interval != lapse_interval_days {
        return Err(format!(
            "a lapsed card should keep a {lapse_interval_days}d interval, got {}",
            lapsed_card.interval
        )
        .into());
    }
    // The relearning step is stored as an absolute timestamp with the same
    // learning fuzz. Unlike the answers above it is not measured from the
    // submitted answer time: grade_now has no answer time to pass on, so
    // upstream stamps the card from its own clock. Bracket that clock instead
    // of the timeline we submitted.
    let lapse_fuzz = ((relearn_secs as f32) * 0.25).min(300.0).floor() as i64;
    let earliest = lapse_started_at / 1000 + relearn_secs as i64;
    let latest = lapse_finished_at / 1000 + relearn_secs as i64 + lapse_fuzz;
    if (lapsed_card.due as i64) < earliest || (lapsed_card.due as i64) > latest {
        return Err(format!(
            "lapse due {} is outside the expected {earliest}..={latest} window for a {relearn_secs}s step plus fuzz",
            lapsed_card.due
        )
        .into());
    }

    let queued = SchedulerService::get_queued_cards(
        &mut collection,
        GetQueuedCardsRequest {
            fetch_limit: 1,
            intraday_learning_only: false,
        },
    )?;
    if queued.new_count != 0 || queued.learning_count != 1 || queued.review_count != 0 {
        return Err(format!(
            "the relearning card should be the only one queued, got new={} learning={} review={}",
            queued.new_count, queued.learning_count, queued.review_count
        )
        .into());
    }
    let queued_card = queued
        .cards
        .first()
        .ok_or("the relearning card was not queued")?;
    let states = queued_card
        .states
        .as_ref()
        .ok_or("queued card missing states")?;
    if state_kind(states.current.as_ref().ok_or("no current state")?) != "relearning" {
        return Err(format!(
            "expected the card to be relearning, got {}",
            state_kind(states.current.as_ref().ok_or("no current state")?)
        )
        .into());
    }
    let Offered::ReviewDays(relearned_days) = offered(states.good.as_ref())? else {
        return Err(format!(
            "the last relearn step answered Good should return a day interval, but Good offered {}",
            state_kind(states.good.as_ref().ok_or("no good state")?)
        )
        .into());
    };
    let fourth_at = TimestampMillis::now().0;
    let changes = SchedulerService::answer_card(
        &mut collection,
        CardAnswer {
            card_id,
            current_state: states.current.clone(),
            new_state: states.good.clone(),
            rating: Rating::Good as i32,
            answered_at_millis: fourth_at,
            milliseconds_taken: 3000,
        },
    )?;
    if !changes.card || !changes.study_queues {
        return Err(
            format!("the relearning answer reported unexpected changes: {changes:?}").into(),
        );
    }
    let relearned_card = upstream_card(&mut collection, card_id)?;
    if relearned_card.ctype != 2 || relearned_card.queue != 2 {
        return Err(format!(
            "a card that finished relearning should be a review card again, got type={} queue={}",
            relearned_card.ctype, relearned_card.queue
        )
        .into());
    }
    if relearned_card.reps != 4 || relearned_card.remaining_steps != 0 {
        return Err(format!(
            "expected four reviews and no steps left, got reps={} remaining_steps={}",
            relearned_card.reps, relearned_card.remaining_steps
        )
        .into());
    }
    if relearned_card.lapses != 1 {
        return Err(format!(
            "recovering from a lapse should not clear it, got lapses={}",
            relearned_card.lapses
        )
        .into());
    }
    let expected_relearn_due = timing.days_elapsed as i32 + relearned_days as i32;
    if relearned_card.interval != relearned_days || relearned_card.due != expected_relearn_due {
        return Err(format!(
            "expected interval {relearned_days} and due {expected_relearn_due} (day {} + {relearned_days}), got interval={} due={}",
            timing.days_elapsed, relearned_card.interval, relearned_card.due
        )
        .into());
    }

    let states = SchedulerService::get_scheduling_states(&mut collection, CardId { cid: card_id })?;
    let current = states
        .current
        .as_ref()
        .ok_or("the recovered card has no current state")?;
    if state_kind(current) != "review" {
        return Err(format!(
            "expected a review card before the Hard answer, got {}",
            state_kind(current)
        )
        .into());
    }
    let hard = states.hard.as_ref().ok_or("no Hard state was offered")?;
    let Offered::ReviewDays(hard_days) = offered(Some(hard))? else {
        return Err(format!(
            "Hard on a review card should offer a day interval, but it offered {}",
            state_kind(hard)
        )
        .into());
    };
    let hard_started_at = TimestampMillis::now().0;
    let changes = SchedulerService::grade_now(
        &mut collection,
        GradeNowRequest {
            card_ids: vec![card_id],
            rating: Rating::Hard as i32,
        },
    )?;
    let hard_finished_at = TimestampMillis::now().0;
    if !changes.card || !changes.study_queues {
        return Err(format!("grade_now reported unexpected changes: {changes:?}").into());
    }
    let hardened_card = upstream_card(&mut collection, card_id)?;
    if hardened_card.ctype != 2 || hardened_card.queue != 2 {
        return Err(format!(
            "Hard on a review card should leave it a review card, got type={} queue={}",
            hardened_card.ctype, hardened_card.queue
        )
        .into());
    }
    if hardened_card.reps != 5 || hardened_card.remaining_steps != 0 {
        return Err(format!(
            "expected five reviews and no steps left, got reps={} remaining_steps={}",
            hardened_card.reps, hardened_card.remaining_steps
        )
        .into());
    }
    if hardened_card.lapses != 1 {
        return Err(format!(
            "Hard is not a lapse, so it should not be counted, got lapses={}",
            hardened_card.lapses
        )
        .into());
    }
    let expected_hard_due = timing.days_elapsed as i32 + hard_days as i32;
    if hardened_card.interval != hard_days || hardened_card.due != expected_hard_due {
        return Err(format!(
            "expected interval {hard_days} and due {expected_hard_due} (day {} + {hard_days}), got interval={} due={}",
            timing.days_elapsed, hardened_card.interval, hardened_card.due
        )
        .into());
    }

    // Close before reading the file directly: the collection holds an exclusive
    // lock, so the raw reader cannot attach while it is open.
    collection.close(None)?;

    let connection = open_raw()?;
    let revlog_rows = rows(&connection, "revlog", &REVLOG_COLUMNS)?;
    let card_rows = rows(&connection, "cards", &CARD_COLUMNS)?;
    drop(connection);
    if revlog_rows.len() != 5 || card_rows.len() != 1 {
        return Err(format!(
            "expected 1 card and 5 revlog rows, got {} and {}",
            card_rows.len(),
            revlog_rows.len()
        )
        .into());
    }

    let learning_ease = expected_ease(&learning_card, fsrs)?.unwrap_or_else(|| "0".to_string());
    let graduated_ease =
        expected_ease(&graduated_card, fsrs)?.unwrap_or_else(|| "2500".to_string());

    let revlog: Vec<&str> = revlog_rows[0].split('|').collect();
    let expected_revlog = [
        first_at.to_string(), // id, the answer time in milliseconds
        card_id.to_string(),
        "-1".to_string(),   // usn, this collection has never synced
        "3".to_string(),    // Good
        format!("-{step}"), // the learning step, in seconds
        "0".to_string(),    // previous interval
        learning_ease,      // ease factor, or FSRS difficulty under FSRS
        "1500".to_string(), // milliseconds_taken we submitted
        "0".to_string(),    // learning review
    ];
    if revlog != expected_revlog {
        return Err(format!(
            "first revlog row {revlog:?} does not match expected {expected_revlog:?}"
        )
        .into());
    }
    let revlog: Vec<&str> = revlog_rows[1].split('|').collect();
    let expected_revlog = [
        second_at.to_string(),
        card_id.to_string(),
        "-1".to_string(),
        "3".to_string(),
        days.to_string(),   // the graduated interval, in whole days
        format!("-{step}"), // the previous interval was still the step
        graduated_ease,
        "2500".to_string(),
        "0".to_string(),
    ];
    if revlog != expected_revlog {
        return Err(format!(
            "second revlog row {revlog:?} does not match expected {expected_revlog:?}"
        )
        .into());
    }

    let lapsed_ease = expected_ease(&lapsed_card, fsrs)?.unwrap_or_else(|| "2300".to_string());
    let revlog: Vec<&str> = revlog_rows[2].split('|').collect();
    // grade_now stamps the row from its own clock, so the id is bracketed by
    // the clock reads either side of the call rather than pinned to a
    // submitted time.
    let lapse_id: i64 = revlog[0].parse()?;
    if lapse_id < lapse_started_at || lapse_id > lapse_finished_at {
        return Err(format!("lapse revlog id {lapse_id} is outside the call window {lapse_started_at}..={lapse_finished_at}").into());
    }
    let expected_revlog = [
        revlog[0].to_string(),
        card_id.to_string(),
        "-1".to_string(),
        "1".to_string(),
        format!("-{relearn_secs}"), // the relearn step, in seconds
        days.to_string(),           // the previous interval was the graduated one
        lapsed_ease,
        "0".to_string(), // grade_now does not record a duration
        "1".to_string(), // a review card was failed
    ];
    if revlog != expected_revlog {
        return Err(format!(
            "third revlog row {revlog:?} does not match expected {expected_revlog:?}"
        )
        .into());
    }

    let relearned_ease =
        expected_ease(&relearned_card, fsrs)?.unwrap_or_else(|| "2300".to_string());
    let revlog: Vec<&str> = revlog_rows[3].split('|').collect();
    let expected_revlog = [
        fourth_at.to_string(),
        card_id.to_string(),
        "-1".to_string(),
        "3".to_string(),            // Good
        relearned_days.to_string(), // the day interval it returned to
        format!("-{relearn_secs}"), // the previous interval was the relearn step
        relearned_ease.clone(),
        "3000".to_string(), // milliseconds_taken we submitted
        "2".to_string(),    // a relearning card was answered
    ];
    if revlog != expected_revlog {
        return Err(format!(
            "fourth revlog row {revlog:?} does not match expected {expected_revlog:?}"
        )
        .into());
    }

    let hardened_ease = expected_ease(&hardened_card, fsrs)?.unwrap_or_else(|| {
        (relearned_ease.parse::<i64>().unwrap_or(2300) - 150).to_string()
    });
    let revlog: Vec<&str> = revlog_rows[4].split('|').collect();
    let hard_id: i64 = revlog[0].parse()?;
    if hard_id < hard_started_at || hard_id > hard_finished_at {
        return Err(format!("Hard revlog id {hard_id} is outside the call window {hard_started_at}..={hard_finished_at}").into());
    }
    let expected_revlog = [
        revlog[0].to_string(),
        card_id.to_string(),
        "-1".to_string(),
        "2".to_string(),            // Hard is the second button
        hard_days.to_string(),      // the day interval Hard scheduled
        relearned_days.to_string(), // the previous interval was the recovered one
        hardened_ease,
        "0".to_string(), // grade_now does not record a duration
        "1".to_string(), // a review card was answered
    ];
    if revlog != expected_revlog {
        return Err(format!(
            "fifth revlog row {revlog:?} does not match expected {expected_revlog:?}"
        )
        .into());
    }

    let persisted_state = persisted_memory_state(&card_rows[0])?;
    match (&persisted_state, fsrs) {
        (Some(state), true) => {
            let recorded = fsrs_memory_state(&hardened_card)?
                .ok_or("the hardened card lost its memory state")?;
            let close = |key: &str, expected: f64| -> Result<bool, Box<dyn std::error::Error>> {
                let value = state
                    .get(key)
                    .and_then(|value| value.as_f64())
                    .ok_or_else(|| format!("persisted memory state is missing {key}: {state}"))?;
                Ok((value - expected).abs()
                    <= (f32::EPSILON as f64) * 2.0 * expected.abs().max(1.0))
            };
            if !close("s", recorded.stability)? || !close("d", recorded.difficulty)? {
                return Err(format!(
                    "persisted memory state {state} disagrees with the backend's own s={} d={}",
                    recorded.stability, recorded.difficulty
                )
                .into());
            }
        }
        (Some(state), false) => {
            return Err(format!("a memory state was persisted without FSRS: {state}").into())
        }
        (None, true) => return Err("FSRS was enabled but nothing was persisted to the card".into()),
        (None, false) => {}
    }

    let persisted = verify_persisted(card_id, &card_rows[0], &revlog_rows)?;
    Ok(format!(
        "PASS upstream Rust review{}\nlearning: {}({}) -> {}({}) step {step}s + {observed_fuzz}s fuzz, due {}\ngraduated: {}({}) interval {days}d due {expected_due}, reps {} -> {}\nlapsed: {}({}) interval {lapse_interval_days}d, {relearn_secs}s relearn step, lapses {} -> {}, due {}\nrelearned: {}({}) interval {relearned_days}d due {expected_relearn_due}, lapses still {}\nhardened: {}({}) interval {hard_days}d due {expected_hard_due}, reps {} -> {}\nrevlog: {}\n        {}\n        {}\n        {}\n        {}\nreopened: {}\ncard: {}",
        if fsrs { " (FSRS enabled)" } else { "" },
        new_card.queue, new_card.ctype, learning_card.queue, learning_card.ctype, learning_card.due,
        graduated_card.queue, graduated_card.ctype, new_card.reps, graduated_card.reps,
        lapsed_card.queue, lapsed_card.ctype, graduated_card.lapses, lapsed_card.lapses, lapsed_card.due,
        relearned_card.queue, relearned_card.ctype, relearned_card.lapses,
        hardened_card.queue, hardened_card.ctype, relearned_card.reps, hardened_card.reps,
        revlog_rows[0], revlog_rows[1], revlog_rows[2], revlog_rows[3], revlog_rows[4], persisted,
        card_rows[0]
    ))
}

fn main() {
    let apt = Apt::new().expect("APT");
    let mut hid = Hid::new().expect("HID");
    let gfx = Gfx::new().expect("GFX");
    let console = Console::new(gfx.top_screen.borrow_mut());
    if let Err(error) = std::fs::create_dir_all(ROOT) {
        eprintln!("could not create {ROOT}: {error}");
    }
    let mut status = review().unwrap_or_else(|error| {
        format!(
            "FAIL: {}\nVFS last Horizon result: 0x{:08x}",
            describe(&*error),
            anki_3ds_sqlite_vfs::last_result()
        )
    });
    if let Err(error) = std::fs::write(RESULT, &status) {
        status = format!("FAIL: evidence write: {error}");
    }
    console.clear();
    while apt.main_loop() {
        gfx.wait_for_vblank();
        println!("\x1b[1;1H{status}\nSTART exits");
        hid.scan_input();
        if hid.keys_down().contains(KeyPad::START) {
            break;
        }
    }
}
