use alloc::boxed::Box;
use core::any::Any;
use core::mem::MaybeUninit;

use crate::abi::*;
#[cfg(feature = "panic-handler")]
pub use crate::panic_handler::*;
use crate::panicking::Exception;

static CANARY: u8 = 0;

#[repr(transparent)]
struct RustPanic(Box<dyn Any + Send>, DropGuard);

struct DropGuard;

impl Drop for DropGuard {
    fn drop(&mut self) {
        #[cfg(feature = "panic-handler")]
        {
            drop_panic();
        }
        crate::util::abort();
    }
}

#[repr(C)]
struct ExceptionWithPayload {
    exception: MaybeUninit<UnwindException>,
    // See rust/library/panic_unwind/src/gcc.rs for the canary values
    canary: *const u8,
    payload: RustPanic,
}

unsafe impl Exception for RustPanic {
    const CLASS: [u8; 8] = *b"MOZ\0RUST";

    fn wrap(this: Self) -> *mut UnwindException {
        Box::into_raw(Box::new(ExceptionWithPayload {
            exception: MaybeUninit::uninit(),
            canary: &CANARY,
            payload: this,
        })) as *mut UnwindException
    }

    unsafe fn unwrap(ex: *mut UnwindException) -> Self {
        let ex = ex as *mut ExceptionWithPayload;
        let canary = unsafe { core::ptr::addr_of!((*ex).canary).read() };
        if !core::ptr::eq(canary, &CANARY) {
            // This is a Rust exception but not generated by us.
            #[cfg(feature = "panic-handler")]
            {
                foreign_exception();
            }
            crate::util::abort();
        }
        let ex = unsafe { Box::from_raw(ex) };
        ex.payload
    }
}

pub fn begin_panic(payload: Box<dyn Any + Send>) -> UnwindReasonCode {
    crate::panicking::begin_panic(RustPanic(payload, DropGuard))
}

pub fn catch_unwind<R, F: FnOnce() -> R>(f: F) -> Result<R, Box<dyn Any + Send>> {
    #[cold]
    fn process_panic(p: Option<RustPanic>) -> Box<dyn Any + Send> {
        match p {
            None => {
                #[cfg(feature = "panic-handler")]
                {
                    foreign_exception();
                }
                crate::util::abort();
            }
            Some(e) => {
                #[cfg(feature = "panic-handler")]
                {
                    panic_caught();
                }
                core::mem::forget(e.1);
                e.0
            }
        }
    }
    crate::panicking::catch_unwind(f).map_err(process_panic)
}
