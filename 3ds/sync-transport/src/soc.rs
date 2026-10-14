// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Explicit ownership of libctru's caller-allocated SOC buffer.
//!
//! libctru socExit() closes shared-memory/service handles but does not free the
//! caller's buffer. Keep the allocation alive until shutdown completes.
use std::alloc::alloc_zeroed;
use std::alloc::dealloc;
use std::alloc::Layout;
use std::io;
use std::ptr::NonNull;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

static SOC_ACTIVE: AtomicBool = AtomicBool::new(false);

extern "C" {
    fn socInit(buffer: *mut u32, size: u32) -> i32;
    fn socExit() -> i32;
}

pub struct NativeSoc {
    buffer: Option<NonNull<u8>>,
    layout: Layout,
}

impl NativeSoc {
    pub fn new() -> io::Result<Self> {
        if SOC_ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(io::Error::other("native SOC is already active"));
        }
        let layout = Layout::from_size_align(1024 * 1024, 4096).unwrap();
        let buffer = NonNull::new(unsafe { alloc_zeroed(layout) }).ok_or_else(|| {
            SOC_ACTIVE.store(false, Ordering::Release);
            io::Error::other("SOC buffer allocation failed")
        })?;
        let result = unsafe { socInit(buffer.as_ptr().cast(), layout.size() as u32) };
        if result < 0 {
            unsafe { dealloc(buffer.as_ptr(), layout) };
            SOC_ACTIVE.store(false, Ordering::Release);
            return Err(io::Error::other(format!("socInit failed: {result}")));
        }
        Ok(Self {
            buffer: Some(buffer),
            layout,
        })
    }

    pub fn close(&mut self) -> io::Result<()> {
        if let Some(buffer) = self.buffer.take() {
            let result = unsafe { socExit() };
            if result < 0 {
                return Err(io::Error::other(format!(
                    "socExit failed; buffer retained: {result}"
                )));
            }
            unsafe { dealloc(buffer.as_ptr(), self.layout) };
            SOC_ACTIVE.store(false, Ordering::Release);
        }
        Ok(())
    }
}

impl Drop for NativeSoc {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            tracing::error!("{error}");
        }
    }
}
