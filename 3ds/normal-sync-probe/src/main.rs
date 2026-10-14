// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

#[cfg(not(target_os = "horizon"))]
mod fixtures;
mod harness;
use std::path::Path;

use harness::ProbeResult;

#[cfg(not(target_os = "horizon"))]
fn main() -> ProbeResult<()> {
    let root = Path::new("out/3ds/normal-sync-probe");
    std::fs::create_dir_all(root)?;
    if std::env::args().nth(1).as_deref() == Some("prepare") {
        fixtures::prepare(&root.join("fixtures"))?;
        return Ok(());
    }
    let run_root = root.join("desktop");
    if run_root.exists() {
        return Err("refusing to overwrite desktop results".into());
    }
    copy_inputs(&root.join("fixtures"), &run_root)?;
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    let output = harness::run(&run_root, |s| println!("{s}"))?;
    std::fs::write(
        root.join("desktop.json"),
        serde_json::to_vec_pretty(&output)?,
    )?;
    println!("disposable_cases_passed={}", harness::CASES.len());
    Ok(())
}

fn copy_inputs(source: &Path, target: &Path) -> ProbeResult<()> {
    std::fs::create_dir_all(target)?;
    for &case in harness::CASES {
        std::fs::create_dir(target.join(case))?;
        for side in ["client", "server"] {
            let file = format!("{side}.anki2");
            let mut reader = std::fs::File::open(source.join(case).join(&file))?;
            let mut writer = std::fs::File::create(target.join(case).join(&file))?;
            std::io::copy(&mut reader, &mut writer)?;
            writer.sync_all()?;
        }
    }
    Ok(())
}

#[cfg(target_os = "horizon")]
const ROOT: &str = "sdmc:/3ds/anki-3ds/phase4-v1";
#[cfg(target_os = "horizon")]
const REPORT: &str = "sdmc:/3ds/anki-normal-sync-probe/report.txt";
#[cfg(target_os = "horizon")]
fn log(text: &str) {
    use std::io::Write;
    println!("{text}");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(REPORT)
        .expect("report");
    writeln!(file, "{text}").expect("write report");
    file.sync_all().expect("flush report");
}

#[cfg(target_os = "horizon")]
fn native_run() -> ProbeResult<()> {
    use anki::collection::CollectionBuilder;
    use anki::sync::login::SyncAuth;
    use anki_3ds_sync_transport::soc::NativeSoc;
    use anki_3ds_sync_transport::NativeSyncClient;
    use ctru::services::ps::Ps;
    use futures::executor::block_on;
    let mut is_new_3ds = false;
    let model_result = unsafe { ctru_sys::APT_CheckNew3DS(&mut is_new_3ds) };
    if model_result < 0 {
        return Err(format!("model query failed: {model_result}").into());
    }
    log(&format!(
        "is_new_3ds={is_new_3ds} model_query_result={model_result} main_stack_bytes=2097152"
    ));
    if is_new_3ds {
        return Err("this verification requires Old 3DS mode".into());
    }
    anki_3ds_sqlite_vfs::register().map_err(|c| format!("VFS registration: {c}"))?;
    let root = Path::new(ROOT);
    let results = root.join("results");
    if results.exists() {
        return Err("results already exist; use a fresh disposable package/profile".into());
    }
    copy_inputs(&root.join("fixtures"), &results)?;
    let output = harness::run(&results, log)?;
    std::fs::write(
        root.join("native.json"),
        serde_json::to_vec_pretty(&output)?,
    )?;
    log(&format!("disposable_cases_passed={}", harness::CASES.len()));
    log(&format!(
        "heap_before_soc={} vfs_flushes={}",
        anki_3ds_sqlite_vfs::heap_used(),
        anki_3ds_sqlite_vfs::flushes()
    ));
    let ps = Ps::new()?;
    let mut soc = NativeSoc::new()?;
    assert!(
        NativeSoc::new().is_err(),
        "SOC owner guard must reject concurrent initialization"
    );
    log("ps=initialized soc=initialized soc_buffer_bytes=1048576");
    let mut col = CollectionBuilder::new(root.join("https-error.anki2")).build()?;
    let before = harness::snapshot(&col)?;
    let client = NativeSyncClient::new(SyncAuth {
        hkey: String::new(),
        endpoint: None,
        io_timeout_secs: Some(60),
    });
    log("chain=Collection::normal_sync_with_client,NormalSyncer::sync,SyncProtocol::meta,NativeSyncClient,curl,mbedTLS,HTTPS endpoint=AnkiWeb unauthenticated=1");
    let result = block_on(col.normal_sync_with_client(client));
    match result {
        Err(anki::error::AnkiError::SyncError { source }) => {
            if !source.info.contains("code: 400") {
                return Err(format!("unexpected HTTPS error: {source:?}").into());
            }
            log("native_chain=PASS_EXPECTED_HTTP_400 transaction_started=0");
        }
        other => return Err(format!("unexpected HTTPS result: {other:?}").into()),
    }
    assert_eq!(
        before,
        harness::snapshot(&col)?,
        "HTTPS rejection modified disposable collection"
    );
    col.close(None)?;
    log(&format!(
        "heap_with_soc={} vfs_flushes={}",
        anki_3ds_sqlite_vfs::heap_used(),
        anki_3ds_sqlite_vfs::flushes()
    ));
    extern "C" {
        fn curl_global_cleanup();
    }
    unsafe { curl_global_cleanup() };
    log("curl_global_cleanup=1");
    soc.close()?;
    drop(soc);
    let mut reinitialized = NativeSoc::new()?;
    reinitialized.close()?;
    log("soc_reinitialize=PASS soc_second_buffer_freed=1");
    drop(ps);
    log(&format!(
        "cleanup=1 soc_closed=1 soc_buffer_freed=1 ps_closed=1 sampled_heap_after_cleanup={}",
        anki_3ds_sqlite_vfs::heap_used()
    ));
    Ok(())
}

#[cfg(target_os = "horizon")]
fn main() {
    use ctru::prelude::*;
    let apt = Apt::new().expect("APT");
    let mut hid = Hid::new().expect("HID");
    let gfx = Gfx::new().expect("GFX");
    let _console = Console::new(gfx.top_screen.borrow_mut());
    std::fs::create_dir_all("sdmc:/3ds/anki-normal-sync-probe").expect("report directory");
    if Path::new(REPORT).exists() {
        std::fs::remove_file(REPORT).expect("previous report");
    }
    log("Anki original NormalSyncer Phase 4 v1");
    use tracing_subscriber::prelude::*;
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .without_time()
                .with_ansi(false)
                .with_writer(|| {
                    std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(REPORT)
                        .expect("trace report")
                })
                .with_filter(
                    tracing_subscriber::filter::Targets::new()
                        .with_target("anki::sync::collection::normal", tracing::Level::DEBUG)
                        .with_target("anki_3ds_sync_transport", tracing::Level::DEBUG),
                ),
        )
        .init();
    match native_run() {
        Ok(()) => log("complete=1 result=PASS"),
        Err(e) => {
            log(&format!("result=FAIL error={e:?}"));
            let mut source = e.source();
            while let Some(cause) = source {
                log(&format!("cause={cause}"));
                source = cause.source();
            }
            log("complete=1 result=FAIL");
        }
    }
    println!("START: exit");
    while apt.main_loop() {
        hid.scan_input();
        if hid.keys_down().contains(KeyPad::START) {
            break;
        }
        gfx.wait_for_vblank();
    }
}
