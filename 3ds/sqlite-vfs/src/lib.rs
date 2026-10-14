// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::os::raw::c_int;

extern "C" {
    fn anki_3ds_vfs_register() -> c_int;
    fn anki_3ds_vfs_flushes() -> u32;
    fn anki_3ds_heap_used() -> u32;
    fn anki_3ds_vfs_last_result() -> u32;
}

/// Register the SD-only VFS before opening any SQLite connection.
pub fn register() -> Result<(), c_int> {
    let code = unsafe { anki_3ds_vfs_register() };
    if code == libsqlite3_sys::SQLITE_OK {
        Ok(())
    } else {
        Err(code)
    }
}

pub fn flushes() -> u32 {
    unsafe { anki_3ds_vfs_flushes() }
}
pub fn heap_used() -> u32 {
    unsafe { anki_3ds_heap_used() }
}

pub fn last_result() -> u32 {
    unsafe { anki_3ds_vfs_last_result() }
}
