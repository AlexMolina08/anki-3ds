Fix collection replacement after full download on Nintendo 3DS. The temporary
download file handle now closes before the SD rename. The previous code retained
that handle; the 3DS filesystem wrapper can obscure a failed file rename with
a subsequent directory-rename “not found” error. Sync file errors now identify
the failed operation and path. The original collection backup and recovery
mechanism remains in place.

Reported on physical hardware after full-download confirmation. Full download
with the patch passed in Old 3DS emulation and matched the original Anki server
collection rows. This patch still requires a physical-console retry.

Install the CIA over the existing application with FBI. Keep
`SD:/3ds/anki-3ds/`; do not delete your collection or saved login.
