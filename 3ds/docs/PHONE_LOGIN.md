# Optional phone login assessment

## Result

A protected local phone companion is technically plausible. It is not built or
verified on a phone or physical Old 3DS. Keep console keyboard login available;
a phone must remain optional for standalone operation.

The current devkitPro headers expose `gethostid()`, `bind()`, `listen()` and
`accept()`. The already-built mbedTLS libraries contain TLS server handshake,
certificate configuration and certificate-generation functions. Their configuration
enables ECDH, AES-GCM and certificate writing. These are useful primitives, not
proof of a working local listener, acceptable memory use or a complete pairing flow.

## Why the plain HTTP browser proposal is insufficient

A QR URL pointing to `http://192.168.x.x:8080` does not authenticate the console
or encrypt credentials. Encrypting the form with JavaScript downloaded over that
same HTTP connection does not protect against an attacker replacing the script.
Web Crypto's `SubtleCrypto` also requires a secure context; a remote LAN HTTP
address is not the phone's trusted localhost exception.

`autocomplete` hints do not establish AnkiWeb identity for a private IP address.
Automatic password-manager selection and biometric autofill need testing, and
must not be promised. Returning HTTP 200 before AnkiWeb authenticates also gives
false success. The saved host key is itself a credential and needs protection.

## Practical candidate

An optional installed phone app could scan a console QR containing its local
address, an ephemeral certificate/public-key fingerprint, and a short-lived random
pairing capability. The phone would authenticate exactly that console key during
TLS, rather than trusting arbitrary self-signed certificates. The console would
accept a bounded login request only during an explicitly opened pairing session,
with expiry, one-time use, size limits, cancellation, and rate limiting.
Credentials would go through the existing upstream host-key exchange. The console
would report success only after server acceptance and token persistence, then
close the listener. The phone would retain no password or token. This requires a
carefully reviewed trust configuration and local-network permission handling;
scanning a QR alone does not implement them.

A browser-only version needs a credible trust/bootstrap design: a browser cannot
pin an arbitrary console key from a QR using ordinary page navigation. Installing
a local CA or accepting certificate warnings would add substantial setup. An
HTTPS-hosted page introduces hosting plus mixed-content/local-network access
constraints. A native companion avoids that web-origin bootstrap problem but adds
phone installation. Neither path is an implemented OAuth/device-code flow.

The smallest next experiment is a temporary TLS listener in a separate `.3dsx`,
strict QR-key verification by a test phone client, one bounded encrypted message,
B-button cancellation and a heap/time report on Old 3DS. Do not add account
credentials until that transport test passes. No listener or new crypto dependency
was added to the application in this change.

## Sources

- [Mbed TLS sample applications](https://mbed-tls.readthedocs.io/en/latest/kb/development/sample_applications/)
- [Apple manual server trust authentication](https://developer.apple.com/documentation/Foundation/performing-manual-server-trust-authentication)
- [MDN SubtleCrypto](https://developer.mozilla.org/en-US/docs/Web/API/SubtleCrypto)
- [MDN secure contexts](https://developer.mozilla.org/en-US/docs/Web/Security/Defenses/Secure_Contexts)
