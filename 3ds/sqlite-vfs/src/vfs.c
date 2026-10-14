// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later.
// SD-only SQLite VFS. All database locks are conservative exclusive directory
// locks, including SHARED requests. Stale locks fail closed after an unclean
// exit; only an offline operator may remove them. No shared-memory WAL support.
#include <3ds.h>
#include <sqlite3.h>
#include <string.h>
#include <stdio.h>
#include <time.h>
#include <stdlib.h>
#include <malloc.h>

#define PATH_LIMIT 512
#define PREFIX "sdmc:/3ds/anki-3ds/"
typedef struct {
    sqlite3_file base;
    Handle handle;
    int lock;
    int delete_on_close;
    char path[PATH_LIMIT];
} SdFile;
static FS_Archive archive;
static int archive_open;
static u32 flush_count;
static Result last_result;
static int failed(Result r) { last_result = r; return R_FAILED(r); }

static int missing(Result r) { return R_SUMMARY(r) == RS_NOTFOUND; }
static FS_Path fs_path(const char *s) { return fsMakePath(PATH_ASCII, s + 5); }
static int valid(const char *s) {
    if (!s) return 0;
    for (const unsigned char *p = (const unsigned char *)s; *p; ++p) {
        if (*p >= 128) return 0;
    }
    return s && strlen(s) < PATH_LIMIT - 8 &&
        strncmp(s, PREFIX, strlen(PREFIX)) == 0 &&
        !strstr(s, "..") && !strstr(s, "//") && !strchr(s, '\\');
}

static void lock_path(SdFile *f, char *out) { strcpy(out, f->path); strcat(out, ".lock"); }
static int unlock(sqlite3_file *p, int level) {
    SdFile *f = (SdFile *)p;
    if (f->lock && level == SQLITE_LOCK_NONE) {
        char path[PATH_LIMIT]; lock_path(f, path);
        if (failed(FSUSER_DeleteDirectory(archive, fs_path(path)))) return SQLITE_IOERR_UNLOCK;
    }
    f->lock = level; return SQLITE_OK;
}
static int close_file(sqlite3_file *p) {
    SdFile *f = (SdFile *)p;
    int rc = failed(FSFILE_Close(f->handle)) ? SQLITE_IOERR_CLOSE : SQLITE_OK;
    // A failed close keeps the lock in place; do not permit another writer.
    if (rc == SQLITE_OK) rc = unlock(p, SQLITE_LOCK_NONE);
    if (rc == SQLITE_OK && f->delete_on_close && failed(FSUSER_DeleteFile(archive, fs_path(f->path)))) rc = SQLITE_IOERR_DELETE;
    p->pMethods = NULL; return rc;
}
static int read_file(sqlite3_file *p, void *buffer, int n, sqlite3_int64 off) {
    u32 count = 0;
    if (off < 0 || failed(FSFILE_Read(((SdFile *)p)->handle, &count, off, buffer, n))) return SQLITE_IOERR_READ;
    if (count != (u32)n) { memset((char *)buffer + count, 0, n - count); return SQLITE_IOERR_SHORT_READ; }
    return SQLITE_OK;
}
static int write_file(sqlite3_file *p, const void *buffer, int n, sqlite3_int64 off) {
    u32 count = 0;
    if (off < 0 || failed(FSFILE_Write(((SdFile *)p)->handle, &count, off, buffer, n, 0)) || count != (u32)n) return SQLITE_IOERR_WRITE;
    return SQLITE_OK;
}
static int truncate_file(sqlite3_file *p, sqlite3_int64 n) {
    return n < 0 || failed(FSFILE_SetSize(((SdFile *)p)->handle, n)) ? SQLITE_IOERR_TRUNCATE : SQLITE_OK;
}
static int sync_file(sqlite3_file *p, int flags) {
    (void)flags;
    if (failed(FSFILE_Flush(((SdFile *)p)->handle))) return SQLITE_IOERR_FSYNC;
    ++flush_count; return SQLITE_OK;
}
static int size_file(sqlite3_file *p, sqlite3_int64 *n) {
    u64 size;
    if (failed(FSFILE_GetSize(((SdFile *)p)->handle, &size))) return SQLITE_IOERR_FSTAT;
    *n = (sqlite3_int64)size; return SQLITE_OK;
}
static int lock_file(sqlite3_file *p, int level) {
    SdFile *f = (SdFile *)p;
    // SQLITE_LOCK_NONE is a request to release, not acquire. It reaches here
    // whenever a transaction ends, so it must not take the lock marker.
    if (level <= f->lock) return SQLITE_OK;
    if (level > SQLITE_LOCK_NONE) {
        char path[PATH_LIMIT]; lock_path(f, path);
        Result created = FSUSER_CreateDirectory(archive, fs_path(path), 0);
        if (R_FAILED(created)) {
            Handle dir;
            Result probed = FSUSER_OpenDirectory(&dir, archive, fs_path(path));
            int owned_by_this_file = R_SUCCEEDED(probed) && f->lock;
            if (R_SUCCEEDED(probed)) FSDIR_Close(dir);
            if (!owned_by_this_file) {
                last_result = created;
                return SQLITE_BUSY;
            }
        }
    }
    f->lock = level; return SQLITE_OK;
}
static int reserved(sqlite3_file *p, int *out) {
    SdFile *f = (SdFile *)p;
    if (f->lock) { *out = f->lock >= SQLITE_LOCK_RESERVED; return SQLITE_OK; }
    char path[PATH_LIMIT]; lock_path(f, path);
    Handle dir; Result r = FSUSER_OpenDirectory(&dir, archive, fs_path(path));
    if (R_SUCCEEDED(r)) { FSDIR_Close(dir); *out = 1; return SQLITE_OK; }
    if (missing(r)) { *out = 0; return SQLITE_OK; }
    last_result = r; return SQLITE_IOERR_CHECKRESERVEDLOCK;
}
static int control(sqlite3_file *p, int op, void *arg) {
    if (op == SQLITE_FCNTL_LOCKSTATE) { *(int *)arg = ((SdFile *)p)->lock; return SQLITE_OK; }
    return SQLITE_NOTFOUND;
}
static int sector(sqlite3_file *p) { (void)p; return 4096; }
static int characteristics(sqlite3_file *p) { (void)p; return 0; }
static const sqlite3_io_methods methods = {
    .iVersion = 1, .xClose = close_file, .xRead = read_file, .xWrite = write_file,
    .xTruncate = truncate_file, .xSync = sync_file, .xFileSize = size_file,
    .xLock = lock_file, .xUnlock = unlock, .xCheckReservedLock = reserved,
    .xFileControl = control, .xSectorSize = sector, .xDeviceCharacteristics = characteristics
};
static int open_file(sqlite3_vfs *v, const char *path, sqlite3_file *p, int flags, int *out) {
    (void)v;
    memset(p, 0, sizeof(SdFile));
    if (!valid(path)) return SQLITE_CANTOPEN;
    SdFile *f = (SdFile *)p;
    u32 mode = FS_OPEN_READ;
    if (flags & SQLITE_OPEN_READWRITE) mode |= FS_OPEN_WRITE;
    if (flags & SQLITE_OPEN_CREATE) mode |= FS_OPEN_CREATE;
    if (flags & SQLITE_OPEN_EXCLUSIVE) {
        if (failed(FSUSER_CreateFile(archive, fs_path(path), 0, 0))) return SQLITE_CANTOPEN;
    }
    if (failed(FSUSER_OpenFile(&f->handle, archive, fs_path(path), mode, 0))) return SQLITE_CANTOPEN;
    strcpy(f->path, path);
    f->delete_on_close = flags & SQLITE_OPEN_DELETEONCLOSE;
    p->pMethods = &methods;
    if (out) *out = flags;
    return SQLITE_OK;
}
static int delete_file(sqlite3_vfs *v, const char *path, int sync_dir) {
    (void)v; (void)sync_dir;
    if (!valid(path)) return SQLITE_IOERR_DELETE;
    Result r = FSUSER_DeleteFile(archive, fs_path(path));
    if (R_SUCCEEDED(r) || missing(r)) return SQLITE_OK;
    last_result = r; return SQLITE_IOERR_DELETE;
}
static int access_file(sqlite3_vfs *v, const char *path, int flags, int *out) {
    (void)v;
    if (!valid(path)) return SQLITE_IOERR_ACCESS;
    Handle h;
    Result r = FSUSER_OpenFile(&h, archive, fs_path(path), FS_OPEN_READ | (flags == SQLITE_ACCESS_READWRITE ? FS_OPEN_WRITE : 0), 0);
    if (R_SUCCEEDED(r)) { FSFILE_Close(h); *out = 1; return SQLITE_OK; }
    if (missing(r)) { *out = 0; return SQLITE_OK; }
    last_result = r; return SQLITE_IOERR_ACCESS;
}
static int full_path(sqlite3_vfs *v, const char *path, int n, char *out) {
    (void)v;
    if (!valid(path) || strlen(path) >= (size_t)n) return SQLITE_CANTOPEN;
    strcpy(out, path); return SQLITE_OK;
}
static int randomness(sqlite3_vfs *v, int n, char *out) {
    (void)v;
    if (failed(psInit())) return 0;
    Result r = PS_GenerateRandomBytes(out, n);
    psExit();
    if (failed(r)) return 0;
    return n;
}
static int sleep_us(sqlite3_vfs *v, int n) { (void)v; svcSleepThread((s64)n * 1000); return n; }
static int current_time(sqlite3_vfs *v, double *out) {
    (void)v; *out = (double)time(NULL) / 86400.0 + 2440587.5; return SQLITE_OK;
}
static int last_error(sqlite3_vfs *v, int n, char *out) {
    (void)v; snprintf(out, n, "Horizon FS result 0x%08lx", (unsigned long)last_result); return (int)last_result;
}
static sqlite3_vfs vfs = {
    .iVersion = 1, .szOsFile = sizeof(SdFile), .mxPathname = PATH_LIMIT,
    .zName = "anki-3ds-sd", .xOpen = open_file, .xDelete = delete_file,
    .xAccess = access_file, .xFullPathname = full_path, .xRandomness = randomness,
    .xSleep = sleep_us, .xCurrentTime = current_time, .xGetLastError = last_error
};
int sqlite3_os_init(void) {
    if (!archive_open) {
        if (failed(FSUSER_OpenArchive(&archive, ARCHIVE_SDMC, fsMakePath(PATH_EMPTY, "")))) return SQLITE_CANTOPEN;
        archive_open = 1;
    }
    return sqlite3_vfs_register(&vfs, 1);
}
int sqlite3_os_end(void) {
    if (archive_open) FSUSER_CloseArchive(archive);
    archive_open = 0; return SQLITE_OK;
}
// SQLite's OS_OTHER default mutex is a no-op. Override it before initialization
// with libctru recursive locks. Do not call pthread from C: pthread-3ds exports
// the Rust libc layout (40 bytes), while newlib's C pthread_mutex_t differs.
struct sqlite3_mutex { RecursiveLock mutex; int dynamic; };
static sqlite3_mutex static_mutexes[12];
static int mutex_init(void) {
    for (int i = 0; i < 12; ++i) RecursiveLock_Init(&static_mutexes[i].mutex);
    return SQLITE_OK;
}
static int mutex_end(void) { return SQLITE_OK; }
static sqlite3_mutex *mutex_alloc(int kind) {
    if (kind >= 2) return kind < 14 ? &static_mutexes[kind - 2] : NULL;
    sqlite3_mutex *m = malloc(sizeof(*m));
    if (!m) return NULL;
    m->dynamic = 1;
    RecursiveLock_Init(&m->mutex);
    return m;
}
static void mutex_free(sqlite3_mutex *m) { if (m && m->dynamic) free(m); }
static void mutex_enter(sqlite3_mutex *m) { if (m) RecursiveLock_Lock(&m->mutex); }
static int mutex_try(sqlite3_mutex *m) { return !m || RecursiveLock_TryLock(&m->mutex) == 0 ? SQLITE_OK : SQLITE_BUSY; }
static void mutex_leave(sqlite3_mutex *m) { if (m) RecursiveLock_Unlock(&m->mutex); }
int anki_3ds_vfs_register(void) {
    static int configured;
    static sqlite3_mutex_methods mutexes = { mutex_init, mutex_end, mutex_alloc,
        mutex_free, mutex_enter, mutex_try, mutex_leave, NULL, NULL };
    if (!configured) {
        int rc = sqlite3_config(SQLITE_CONFIG_MUTEX, &mutexes);
        if (rc != SQLITE_OK) return rc;
        configured = 1;
    }
    return sqlite3_initialize();
}
u32 anki_3ds_vfs_flushes(void) { return flush_count; }
u32 anki_3ds_vfs_last_result(void) { return last_result; }
u32 anki_3ds_heap_used(void) { return mallinfo().uordblks; }
