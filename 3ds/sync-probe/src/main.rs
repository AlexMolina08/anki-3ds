// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Native Anki 3DS sync integration probe.
//! Tests real upstream SyncProtocol trait methods running on ARMv6K
//! through NativeSyncClient and native HTTPS transport.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::time::Instant;

use anki::sync::collection::meta::MetaRequest;
use anki::sync::collection::protocol::SyncProtocol;
use anki::sync::login::{HostKeyRequest, SyncAuth};
use anki::sync::request::IntoSyncRequest;
use anki_3ds_sync_transport::NativeSyncClient;
use ctru::prelude::*;
use ctru::services::ps::Ps;
use ctru::services::soc::Soc;
use futures::executor::block_on;
use http::StatusCode;

const REPORT: &str = "sdmc:/3ds/anki-sync-probe/report.txt";

fn log_event(text: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(REPORT) {
        let _ = writeln!(f, "{}", text);
        let _ = f.sync_all();
    }
}

fn main() {
    let apt = Apt::new().expect("Failed to initialize APT");
    let mut hid = Hid::new().expect("Failed to initialize HID");
    let gfx = Gfx::new().expect("Failed to initialize GFX");
    let _console = Console::new(gfx.top_screen.borrow_mut());

    println!("Anki 3DS Upstream SyncEngine Probe");
    println!("Initializing SOC, PS and SD storage...");

    let _ = fs::create_dir_all("sdmc:/3ds/anki-sync-probe");
    let _ = fs::remove_file(REPORT);

    log_event("Anki 3DS Upstream SyncEngine Probe v2");
    log_event("target=armv6k-nintendo-3ds sync_trait=SyncProtocol transport=curl+mbedTLS adapter=NativeSyncClient");

    let _ps = match Ps::new() {
        Ok(ps) => {
            println!("PS initialized successfully");
            log_event("ps=initialized");
            Some(ps)
        }
        Err(e) => {
            println!("PS initialization failed: {:?}", e);
            log_event(&format!("ps_error={:?}", e));
            None
        }
    };

    // Allocate 1MB SOC buffer aligned to 0x1000
    let soc = match Soc::new() {
        Ok(soc) => {
            println!("SOC initialized successfully");
            log_event("soc=initialized");
            Some(soc)
        }
        Err(e) => {
            println!("SOC initialization failed: {:?}", e);
            log_event(&format!("soc_error={:?}", e));
            None
        }
    };

    if soc.is_some() && _ps.is_some() {
        let mut wifi_wait_start = Instant::now();
        unsafe {
            while ctru_sys::osGetWifiStrength() == 0 && wifi_wait_start.elapsed().as_secs() < 10 {
                println!("Waiting for Wi-Fi link...");
                gfx.wait_for_vblank();
            }
            let strength = ctru_sys::osGetWifiStrength();
            println!("Wi-Fi link strength: {}/3", strength);
            log_event(&format!("wifi_strength={}", strength));
        }

        println!("\n[Stage A: Upstream SyncProtocol::meta]");
        println!("Constructing upstream MetaRequest::request()...");

        let client = NativeSyncClient::new(SyncAuth {
            hkey: String::new(),
            endpoint: None,
            io_timeout_secs: Some(60),
        });

        let meta_req = MetaRequest::request();
        println!("Client version: {}", meta_req.client_version);
        println!("Sync version: {}", meta_req.sync_version.0);

        let start = Instant::now();
        let meta_res = block_on(client.meta(meta_req));
        let elapsed = start.elapsed();

        match meta_res {
            Ok(resp) => {
                println!("Unexpected Ok from unauthenticated meta: {:?}", resp);
                log_event(&format!("meta_result=OK_UNEXPECTED elapsed_ms={}", elapsed.as_millis()));
            }
            Err(http_err) => {
                println!("Response in {:?}", elapsed);
                println!("Upstream HttpError: code={} context={}", http_err.code, http_err.context);
                log_event(&format!(
                    "meta: code={} elapsed_ms={} context={}",
                    http_err.code.as_u16(),
                    elapsed.as_millis(),
                    http_err.context.trim()
                ));

                if http_err.code == StatusCode::BAD_REQUEST {
                    println!("\nPASS: Received expected HTTP 400 through upstream HttpError!");
                    println!("Genuine upstream SyncProtocol trait executed on ARMv6K.");
                    log_event("meta_result=PASS_UPSTREAM_HTTP_400");
                } else {
                    println!("\nReceived status: {}", http_err.code);
                    log_event(&format!("meta_result=STATUS_{}", http_err.code.as_u16()));
                }
            }
        }

        println!("\n[Stage B: Upstream SyncProtocol::host_key]");
        println!("Testing upstream HostKeyRequest serialization...");
        let hostkey_req = HostKeyRequest {
            username: "probe_unauthenticated_test_user".to_string(),
            password: "invalid_test_password".to_string(),
        }
        .try_into_sync_request()
        .expect("infallible host key request");

        let start_login = Instant::now();
        let login_res = block_on(client.host_key(hostkey_req));
        let elapsed_login = start_login.elapsed();

        match login_res {
            Ok(resp) => {
                println!("Unexpected login success: {:?}", resp);
                log_event("login_result=OK_UNEXPECTED");
            }
            Err(http_err) => {
                println!("Login response in {:?}: HTTP {}", elapsed_login, http_err.code);
                println!("Context: {}", http_err.context);
                log_event(&format!(
                    "login: code={} elapsed_ms={} context={}",
                    http_err.code.as_u16(),
                    elapsed_login.as_millis(),
                    http_err.context.trim()
                ));
                if http_err.code == StatusCode::FORBIDDEN || http_err.code == StatusCode::BAD_REQUEST {
                    println!("PASS: Authentication rejection cleanly mapped to upstream HttpError!");
                    log_event("login_result=PASS_UPSTREAM_ERROR_MAPPED");
                }
            }
        }
    }

    log_event("complete=1");
    println!("\nDone! Results saved to SD report.");
    println!("Press START to exit safely.");

    while apt.main_loop() {
        hid.scan_input();
        if hid.keys_down().contains(KeyPad::START) {
            break;
        }
        gfx.wait_for_vblank();
    }

    log_event("cleanup=1");
    println!("Exiting cleanly...");
}
