# Copyright: Anki 3DS contributors
# License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

"""Continue a 3DS FSRS review with a desktop Anki backend.

Reading the collection back only proves the bytes parse. This replays the same
review history in a desktop-only collection and compares the resulting memory
state, then schedules one more review from it. That is the compatibility claim
that matters: the state the console wrote must be one the desktop scheduler
treats exactly as its own.

Run against this checkout's freshly built backend rather than the installed
app's: the app bundles an older fsrs than the version pinned here, and the two
disagree on the short-term stability update for Hard, so only the same version
can be compared.

Reads a copy, so the SD file stays exactly as the console left it.
"""
import json
import shutil
import tempfile
import time
from pathlib import Path

import anki.scheduler_pb2
from anki.collection import Collection

root = Path(__file__).resolve().parents[2]
source = root / "out/3ds/emulator/data/azahar-emu/sdmc/3ds/anki-3ds/disposable.anki2"
target = root / f"out/3ds/fixtures/fsrs-continuation-{time.time_ns()}.anki2"
if target.exists():
    raise RuntimeError(f"Refusing to overwrite {target}")
for suffix in ("-wal", "-shm"):
    if Path(str(source) + suffix).exists():
        raise RuntimeError(f"Close the probe and checkpoint the collection ({suffix} present)")
shutil.copy2(source, target)

CONSOLE_ANSWERS = 5


def memory_state(collection):
    """The FSRS memory state, without the clock-dependent fields."""
    data = json.loads(collection.db.scalar("select data from cards"))
    return {key: data[key] for key in ("s", "d", "dr", "decay") if key in data}


def card_row(collection):
    """The scheduling state, without the fields that track the wall clock.

    `due` is deliberately excluded: for a card in relearning it is an absolute
    timestamp, so the console's emulated clock and the desktop's real one will
    never agree on it even when the scheduling decision is identical.
    """
    return list(collection.db.first("select type, queue, ivl, reps, lapses, left from cards"))


def answer_good(collection, card_id):
    """Answer Good with the state the backend itself offers, as the UI does."""
    backend = collection._backend
    queued = backend.get_queued_cards(fetch_limit=1, intraday_learning_only=False)
    assert len(queued.cards) == 1, f"expected one queued card, got {len(queued.cards)}"
    card = queued.cards[0]
    assert card.card.id == card_id, (card.card.id, card_id)
    changes = backend.answer_card(
        anki.scheduler_pb2.CardAnswer(
            card_id=card_id,
            current_state=card.states.current,
            new_state=card.states.good,
            rating=anki.scheduler_pb2.CardAnswer.GOOD,
            answered_at_millis=int(time.time() * 1000),
            milliseconds_taken=1000,
        )
    )
    assert changes.card, changes


def answer_again(collection, card_id):
    """Lapse the card with the backend's own grade-now entry point.

    The console cannot answer an undue review card through the queue either, so
    it uses the same call.
    """
    changes = collection._backend.grade_now(
        card_ids=[card_id], rating=anki.scheduler_pb2.CardAnswer.AGAIN
    )
    assert changes.card, changes


def answer_hard(collection, card_id):
    """Answer Hard with grade_now, the same way the console answers an undue
    review card. Hard is not a lapse, so it must not touch the lapse counter."""
    changes = collection._backend.grade_now(
        card_ids=[card_id], rating=anki.scheduler_pb2.CardAnswer.HARD
    )
    assert changes.card, changes


def make_due_now(collection):
    """Bring the card forward instead of waiting out its interval.

    A card in learning is stored with an absolute timestamp, but a review card
    is stored with a day number, so the two need different values to be in the
    past.
    """
    ctype = collection.db.scalar("select type from cards")
    if ctype == 2:  # CardType.REVIEW
        collection.db.execute("update cards set due = 0")
    else:
        collection.db.execute("update cards set due = ?", int(time.time()) - 1)


console = Collection(str(target))
assert console.get_config("fsrs") is True, "FSRS was not persisted by the console"
assert console.db.scalar("select count(*) from revlog") == CONSOLE_ANSWERS
card_id = console.find_cards("")[0]
console_state = memory_state(console)
assert console_state, "the console wrote no memory state"
console_after_history = card_row(console)
assert console_after_history[0] == 2, f"the console should have recovered the card: {console_after_history}"
assert console_after_history[4] == 1, f"the lapse should still be counted: {console_after_history}"

scratch = Path(tempfile.mkdtemp())
baseline = Collection(str(scratch / "baseline.anki2"))
baseline.set_config("fsrs", True)
note = baseline.new_note(baseline.models.by_name("Basic"))
note["Front"] = "Old 3DS SQLite fixture"
note["Back"] = "Generated by upstream Anki"
baseline.add_note(note, 1)
baseline_card_id = baseline.find_cards("")[0]
answer_good(baseline, baseline_card_id)
answer_good(baseline, baseline_card_id)
answer_again(baseline, baseline_card_id)
answer_good(baseline, baseline_card_id)
answer_hard(baseline, baseline_card_id)

assert memory_state(baseline) == console_state, (
    f"console wrote {console_state}, desktop computed {memory_state(baseline)}"
)
assert card_row(baseline) == console_after_history, (
    f"console {console_after_history}, desktop {card_row(baseline)}"
)

make_due_now(console)
answer_good(console, card_id)
console_after_extra = memory_state(console)

make_due_now(baseline)
answer_good(baseline, baseline_card_id)
baseline_after_extra = memory_state(baseline)

assert console_after_extra == baseline_after_extra, (
    f"after the review console={console_after_extra} desktop={baseline_after_extra}"
)
assert console_after_extra["d"] < console_state["d"], (console_state, console_after_extra)
assert console_after_extra["s"] >= console_state["s"], (console_state, console_after_extra)

assert console.db.scalar("select count(*) from revlog") == CONSOLE_ANSWERS + 1
assert console.db.scalar("PRAGMA quick_check") == "ok"
baseline.close()
console.close()

result = {
    "verified_copy": str(target),
    "desktop_continued_console_fsrs_review": True,
    "console_answers_replayed": CONSOLE_ANSWERS,
    "memory_state_after_console_history": console_state,
    "memory_state_after_extra_review": console_after_extra,
    "console_matches_desktop_scheduler": True,
    "revlog_rows": CONSOLE_ANSWERS + 1,
}
(root / "out/3ds/logs/fsrs-continuation.json").write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps(result))
