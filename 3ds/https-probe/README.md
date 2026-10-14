# Old 3DS secure HTTPS probe

This separate homebrew application tests direct HTTPS. It does not open an Anki collection, log in, or synchronize. Do not enter account credentials.

## Install and run

1. Extract the hardware ZIP. Copy its `3ds` folder to the root of the console SD card, merging folders.
2. Confirm these files exist:
   - `/3ds/anki-https-probe/anki-https-probe.3dsx`
   - `/3ds/anki-https-probe/cacert.pem`
3. Configure working Wi-Fi through the console's normal Internet Settings. Set the console date and time correctly; certificate validity depends on its clock.
4. Open Homebrew Launcher and launch **Anki HTTPS probe**. No computer connection is needed.
5. Keep the console awake. The screen names each test. A full run has seven requests; allow up to four minutes.
6. Press **B** to cancel. Press **START** to cancel pending work and exit after resources are released. Do not remove the SD card while running.
7. After `Done: ...; SD saved`, press START. Retrieve `/3ds/anki-https-probe/report.txt` from the SD card and send that file back.

The report contains public DNS results, TLS version, certificate result, HTTP status, timings, byte counts, errors, and sampled allocated heap. No credentials or response bodies are saved. A public binary response hash is recorded. Each run replaces the previous report; copy it elsewhere before another run if you need both.

An unauthenticated HTTP error from AnkiWeb can pass the transport test. It does **not** mean synchronization works. Expired-certificate and wrong-hostname tests pass only when the connection is rejected. The binary echo and 32-byte tests require their expected payload evidence. The final 1 ms deadline test must fail with a timeout or cancellation error.

If a test fails, keep the report. Never disable certificate validation. An SD write failure appears on screen; a missing or incomplete report is not a successful run.

## Build and emulator verification

From this repository:

```sh
just old3ds-https-libs
just old3ds-https-build
just old3ds-https-emulator
just old3ds-https-package
```

Libraries and outputs stay under `out/3ds/`. No system packages are replaced. Build verifies ELF ARMv6K/VFPv2 attributes and runs bounded DNS parser tests. See `build-libs.sh` for pinned source archives and target-specific compatibility changes.

Azahar runs in a separate Old 3DS profile under `out/3ds/https-probe/emulator`. Azahar stubs Horizon's DNS-table query, so the driver writes an explicit DNS override using the Mac's configured IPv4 DNS. This is emulator-only configuration. The hardware ZIP deliberately excludes `dns-server.txt`; physical testing uses the console's configured DNS servers. No proxy is used.

Native DNS uses bounded UDP queries, with 5 seconds per configured server, at most two servers, and 100 ms cancellation polling. Truncated DNS responses requiring TCP fail explicitly. Redirects are reported but not followed in this experiment: a future adapter must resolve every redirect hostname with the same bounds.

HTTP uses a dedicated worker, strict TLS 1.2 minimum, certificate-chain and hostname checks, HTTP/1.1, 10-second connect and 20-second request deadlines, an 8 KiB curl receive buffer, and a 256 KiB response cap. Only a 4 KiB prefix is retained temporarily for binary echo verification. The main UI thread remains responsive. TLS randomness comes from `PS_GenerateRandomBytes`; service failure stops the test.

The bundled Mozilla trust store comes from curl's CA extract service. Its original date and license text remain in `cacert.pem`. TLS security depends on maintaining this bundle and the libraries; this prototype does not automatically update either. SD files are user-editable, as usual for homebrew.

Emulator results do not establish Old 3DS hardware compatibility. Authentication and sync-engine changes wait for the physical report.
