# Horizon SD SQLite VFS

This adapter uses upstream libsqlite3-sys 0.34.0 and its unchanged bundled SQLite
3.49.2, with `SQLITE_OS_OTHER=1` and `SQLITE_TEMP_STORE=3`. The application must
call `register()` before using SQLite. That installs real libctru recursive
mutexes; the OS_OTHER default no-op mutex implementation is never used at runtime.

Native FS services provide offset reads/writes, truncation, size, close, and
`FSFILE_Flush`. Paths must be absolute ASCII paths below `sdmc:/3ds/anki-3ds/`.
Relative paths, parent traversal, unnamed disk temporary files, shared-memory
WAL, and memory mapping are unsupported. Temporary SQLite data uses RAM.

Each database uses an atomic native directory creation as a conservative
exclusive lock, even when SQLite requests SHARED. Competing connections receive
BUSY. All clients must use this VFS; external clients ignoring these directories
are not protected. WAL requires `locking_mode=EXCLUSIVE` before first access,
matching Anki's current configuration. No scheduling or schema code is replaced.

A crash leaves a `.lock` directory. The VFS deliberately fails closed. An offline
operator may remove an empty stale directory only after proving no client is
running; never remove one while a database is open. The emulator test harness
performs this explicit recovery only for its own disposable probe database.

This is an experimental adapter, not a hardware durability guarantee. Native
file flush calls are checked. The SD service exposes no directory fsync here;
power-loss ordering of create/delete metadata remains unverified. No atomic-write
or safe-append device capabilities are advertised. Old 3DS hardware, SD removal,
full media, and concurrent access by applications using other VFSes require
separate validation.
