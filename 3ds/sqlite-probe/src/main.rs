// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::fmt::Write as _;

use ctru::prelude::*;
use rusqlite::Connection;
use rusqlite::OpenFlags;

const ROOT: &str = "sdmc:/3ds/anki-3ds";
const DB: &str = "sdmc:/3ds/anki-3ds/probe.sqlite";

fn open(path: &str) -> rusqlite::Result<Connection> {
    let db = Connection::open_with_flags_and_vfs(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        "anki-3ds-sd",
    )?;
    db.busy_timeout(std::time::Duration::ZERO)?;
    db.execute_batch(
        "PRAGMA locking_mode=EXCLUSIVE; PRAGMA temp_store=MEMORY; PRAGMA synchronous=FULL;",
    )?;
    Ok(db)
}

fn run() -> Result<String, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(ROOT)?;
    anki_3ds_sqlite_vfs::register().map_err(|code| format!("VFS init: {code}"))?;
    let heap_before = anki_3ds_sqlite_vfs::heap_used();
    let mut db = open(DB)?;
    db.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;
        CREATE TABLE IF NOT EXISTS proof(id INTEGER PRIMARY KEY, launches INTEGER NOT NULL);
        INSERT OR IGNORE INTO proof VALUES(1,0);",
    )?;
    let mode: String = db.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
    assert_eq!(mode, "wal");
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='uncommitted_payload'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        0
    );
    let before: i64 = db.query_row("SELECT launches FROM proof WHERE id=1", [], |r| r.get(0))?;
    {
        let tx = db.transaction()?;
        tx.execute("UPDATE proof SET launches=launches+1 WHERE id=1", [])?;
        tx.commit()?;
    }
    {
        let tx = db.transaction()?;
        tx.execute("UPDATE proof SET launches=999999 WHERE id=1", [])?;
        tx.rollback()?;
    }
    let after: i64 = db.query_row("SELECT launches FROM proof WHERE id=1", [], |r| r.get(0))?;
    assert_eq!(after, before + 1);
    assert!(std::fs::metadata(format!("{DB}-wal"))?.len() > 0);
    let other =
        Connection::open_with_flags_and_vfs(DB, OpenFlags::SQLITE_OPEN_READ_WRITE, "anki-3ds-sd")?;
    other.busy_timeout(std::time::Duration::ZERO)?;
    let error = other
        .query_row("SELECT launches FROM proof", [], |r| r.get::<_, i64>(0))
        .unwrap_err();
    assert!(
        matches!(error, rusqlite::Error::SqliteFailure(e, _) if e.code == rusqlite::ErrorCode::DatabaseBusy)
    );
    drop(other);
    assert!(Connection::open_with_flags_and_vfs(
        "sdmc:/3ds/anki-3ds/missing/no.sqlite",
        OpenFlags::SQLITE_OPEN_READ_WRITE,
        "anki-3ds-sd"
    )
    .is_err());
    assert!(open("sdmc:/3ds/anki-3ds/../forbidden.sqlite").is_err());
    assert!(open("sdmc:/3ds/anki-3ds/é.sqlite").is_err());
    let crash = std::fs::metadata(format!("{ROOT}/crash-request")).is_ok();
    if crash {
        // Leave committed WAL and an uncommitted transaction for restart
        // recovery.
        db.execute_batch(
            "PRAGMA cache_size=4; BEGIN IMMEDIATE;
            UPDATE proof SET launches=999999 WHERE id=1;
            CREATE TABLE uncommitted_payload(id INTEGER PRIMARY KEY, data BLOB);",
        )?;
        for id in 0..32 {
            db.execute(
                "INSERT INTO uncommitted_payload VALUES(?,zeroblob(8192))",
                [id],
            )?;
        }
        assert!(std::fs::metadata(format!("{DB}-wal"))?.len() > 200_000);
        std::mem::forget(db);
    } else {
        db.close().map_err(|(_, e)| e)?;
        let reopened = open(DB)?;
        assert_eq!(
            reopened.query_row("SELECT launches FROM proof", [], |r| r.get::<_, i64>(0))?,
            after
        );
        assert_eq!(
            reopened.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))?,
            "ok"
        );
        reopened.close().map_err(|(_, e)| e)?;
        let readonly = Connection::open_with_flags_and_vfs(
            DB,
            OpenFlags::SQLITE_OPEN_READ_ONLY,
            "anki-3ds-sd",
        )?;
        readonly.execute_batch("PRAGMA locking_mode=EXCLUSIVE")?;
        let err = readonly
            .execute("UPDATE proof SET launches=0", [])
            .unwrap_err();
        assert!(
            matches!(err, rusqlite::Error::SqliteFailure(e, _) if e.code == rusqlite::ErrorCode::ReadOnly)
        );
        readonly.close().map_err(|(_, e)| e)?;
        std::fs::write(
            format!("{ROOT}/corrupt.sqlite"),
            b"This is not a SQLite database",
        )?;
        let corrupt = open(&format!("{ROOT}/corrupt.sqlite"));
        assert!(corrupt.is_err());
        // Exercise rollback-journal creation and rollback independently of WAL.
        let mut journal = open("sdmc:/3ds/anki-3ds/journal.sqlite")?;
        journal.execute_batch("PRAGMA journal_mode=DELETE; CREATE TABLE IF NOT EXISTS test(value INTEGER); DELETE FROM test; INSERT INTO test VALUES(7);")?;
        let tx = journal.transaction()?;
        tx.execute("UPDATE test SET value=8", [])?;
        assert!(std::fs::metadata(format!("{ROOT}/journal.sqlite-journal")).is_ok());
        tx.rollback()?;
        assert_eq!(
            journal.query_row("SELECT value FROM test", [], |r| r.get::<_, i64>(0))?,
            7
        );
        journal.close().map_err(|(_, e)| e)?;
    }
    let collection_status = if crash {
        "Collection test skipped during crash arm".to_string()
    } else {
        verify_collection()?
    };
    let mut result = String::new();
    writeln!(result, "PASS SQLite {}", rusqlite::version())?;
    writeln!(
        result,
        "Previous launches: {before}\nCommitted launches: {after}"
    )?;
    writeln!(result, "WAL / FULL sync / rollback / BUSY")?;
    writeln!(result, "Crash recovery armed: {crash}")?;
    writeln!(
        result,
        "Native FS flushes: {}",
        anki_3ds_sqlite_vfs::flushes()
    )?;
    writeln!(
        result,
        "Heap used: {heap_before} -> {}",
        anki_3ds_sqlite_vfs::heap_used()
    )?;
    writeln!(result, "SQLite peak bytes: {}", unsafe {
        rusqlite::ffi::sqlite3_memory_highwater(0)
    })?;
    writeln!(result, "{collection_status}")?;
    Ok(result)
}

fn verify_collection() -> Result<String, Box<dyn std::error::Error>> {
    let path = "sdmc:/3ds/anki-3ds/disposable.anki2";
    if std::fs::metadata(path).is_err() {
        return Ok("Anki fixture absent: not tested".to_string());
    }
    let mut db = Connection::open_with_flags_and_vfs(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE,
        "anki-3ds-sd",
    )?;
    db.create_collation("unicase", |a, b| {
        unicase::UniCase::new(a).cmp(&unicase::UniCase::new(b))
    })?;
    // Match original storage/sqlite.rs's exclusive WAL setup. Temporary data
    // uses RAM because this VFS has no unnamed disk temporary files.
    db.execute_batch(
        "PRAGMA locking_mode=EXCLUSIVE; PRAGMA page_size=4096;
        PRAGMA cache_size=-40960; PRAGMA journal_mode=WAL;
        PRAGMA temp_store=MEMORY; PRAGMA synchronous=FULL;",
    )?;
    let version: i64 = db.query_row("SELECT ver FROM col", [], |r| r.get(0))?;
    assert_eq!(version, 18);
    let get_note = include_str!("../../../rslib/src/storage/note/get.sql");
    let (id, fields, tags): (i64, String, String) =
        db.query_row(get_note, [], |r| Ok((r.get(0)?, r.get(6)?, r.get(5)?)))?;
    assert_eq!(
        fields,
        "Old 3DS SQLite fixture\x1fGenerated by upstream Anki"
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM notes", [], |r| r.get::<_, i64>(0))?,
        1
    );
    let card_note: i64 = db.query_row(
        include_str!("../../../rslib/src/storage/card/get_card.sql"),
        [],
        |r| r.get(1),
    )?;
    assert_eq!(card_note, id);
    let update = include_str!("../../../rslib/src/storage/note/update_tags.sql");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;
    {
        let tx = db.transaction()?;
        assert_eq!(
            tx.execute(update, rusqlite::params![now, -1, " rollback_only ", id])?,
            1
        );
        tx.rollback()?;
    }
    assert_eq!(db.query_row(get_note, [], |r| r.get::<_, String>(5))?, tags);
    {
        let tx = db.transaction()?;
        assert_eq!(
            tx.execute(update, rusqlite::params![now, -1, " old3ds_probe ", id])?,
            1
        );
        tx.commit()?;
    }
    assert_eq!(
        db.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))?,
        "ok"
    );
    db.close().map_err(|(_, e)| e)?;
    let reopened = open(path)?;
    assert_eq!(
        reopened.query_row(get_note, [], |r| r.get::<_, String>(5))?,
        " old3ds_probe "
    );
    reopened.close().map_err(|(_, e)| e)?;
    Ok(format!(
        "Anki schema {version}: note/card SQL PASS\nPrior fixture tags: {tags:?}"
    ))
}

fn main() {
    let apt = Apt::new().expect("APT");
    let mut hid = Hid::new().expect("HID");
    let gfx = Gfx::new().expect("GFX");
    let console = Console::new(gfx.top_screen.borrow_mut());
    let mut status = match run() {
        Ok(s) => s,
        Err(e) => format!("FAIL: {e}"),
    };
    if let Err(error) = std::fs::write(format!("{ROOT}/result.txt"), &status) {
        status = format!("FAIL: cannot write result: {error}");
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
