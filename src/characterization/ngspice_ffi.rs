//! Minimal hand-written FFI bindings to libngspice's shared-library API
//! (`sharedspice.h`), covering only the functions/callbacks this crate
//! actually uses: `ngSpice_Init`, `ngSpice_Command`, `ngGet_Vec_Info`.
//!
//! Field layouts and callback signatures below were confirmed against the
//! installed header at `/usr/include/ngspice/sharedspice.h`.

use std::ffi::{c_char, c_int, c_short, c_void, CStr, CString};
use std::sync::{Mutex, OnceLock};

/// Mirrors `vector_info` from sharedspice.h.
#[repr(C)]
struct VectorInfo {
    v_name: *mut c_char,
    v_type: c_int,
    v_flags: c_short,
    v_realdata: *mut f64,
    v_compdata: *mut c_void, // ngcomplex_t*, unused by this crate
    v_length: c_int,
}

// Opaque, never dereferenced: only used so the SendData/SendInitData
// callback shims have ABI-correct pointer parameter types.
#[repr(C)]
struct VecValuesAll {
    _private: [u8; 0],
}
#[repr(C)]
struct VecInfoAll {
    _private: [u8; 0],
}

type SendChar = unsafe extern "C" fn(*mut c_char, c_int, *mut c_void) -> c_int;
type SendStat = unsafe extern "C" fn(*mut c_char, c_int, *mut c_void) -> c_int;
type ControlledExit = unsafe extern "C" fn(c_int, c_int, c_int, c_int, *mut c_void) -> c_int;
type SendData = unsafe extern "C" fn(*mut VecValuesAll, c_int, c_int, *mut c_void) -> c_int;
type SendInitData = unsafe extern "C" fn(*mut VecInfoAll, c_int, *mut c_void) -> c_int;
type BgThreadRunning = unsafe extern "C" fn(c_int, c_int, *mut c_void) -> c_int;

unsafe extern "C" {
    fn ngSpice_Init(
        printfcn: Option<SendChar>,
        statfcn: Option<SendStat>,
        exitfcn: Option<ControlledExit>,
        sdata: Option<SendData>,
        sinitdata: Option<SendInitData>,
        bgtrun: Option<BgThreadRunning>,
        user_data: *mut c_void,
    ) -> c_int;

    fn ngSpice_Command(command: *mut c_char) -> c_int;

    fn ngGet_Vec_Info(vecname: *mut c_char) -> *mut VectorInfo;
}

/// Global text buffer that `send_char` appends into. Safe as a single
/// process-wide buffer because this CLI is single-threaded and only ever
/// runs one ngspice session at a time.
static OUTPUT_BUFFER: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

fn output_buffer() -> &'static Mutex<Vec<String>> {
    OUTPUT_BUFFER.get_or_init(|| Mutex::new(Vec::new()))
}

unsafe extern "C" fn send_char(text: *mut c_char, _id: c_int, _user: *mut c_void) -> c_int {
    if !text.is_null() {
        let line = unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned();
        output_buffer()
            .lock()
            .expect("ngspice output buffer mutex poisoned")
            .push(line);
    }
    0
}

unsafe extern "C" fn send_stat(_text: *mut c_char, _id: c_int, _user: *mut c_void) -> c_int {
    0
}

unsafe extern "C" fn controlled_exit(
    _status: c_int,
    _unload_immediately: c_int,
    _exit_on_quit: c_int,
    _id: c_int,
    _user: *mut c_void,
) -> c_int {
    0
}

unsafe extern "C" fn send_data(
    _data: *mut VecValuesAll,
    _count: c_int,
    _id: c_int,
    _user: *mut c_void,
) -> c_int {
    0
}

unsafe extern "C" fn send_init_data(
    _data: *mut VecInfoAll,
    _id: c_int,
    _user: *mut c_void,
) -> c_int {
    0
}

unsafe extern "C" fn bg_thread_running(_running: c_int, _id: c_int, _user: *mut c_void) -> c_int {
    0
}

#[derive(Debug, thiserror::Error)]
pub enum FfiError {
    #[error("ngSpice_Init failed (returned {0})")]
    InitFailed(c_int),
    #[error("ngSpice_Command('{0}') failed (returned {1})")]
    CommandFailed(String, c_int),
    #[error("vector '{0}' is not available or has zero length")]
    VectorUnavailable(String),
    #[error("command string '{0}' contains an interior NUL byte")]
    NulInCommand(String),
}

/// A live ngspice shared-library session. Only one should exist per process
/// (the underlying library has global state).
pub struct NgspiceSession {
    _private: (),
}

impl NgspiceSession {
    pub fn start() -> Result<Self, FfiError> {
        let rc = unsafe {
            ngSpice_Init(
                Some(send_char),
                Some(send_stat),
                Some(controlled_exit),
                Some(send_data),
                Some(send_init_data),
                Some(bg_thread_running),
                std::ptr::null_mut(),
            )
        };
        if rc != 0 {
            return Err(FfiError::InitFailed(rc));
        }
        Ok(NgspiceSession { _private: () })
    }

    pub fn command(&self, cmd: &str) -> Result<(), FfiError> {
        let c_cmd = CString::new(cmd).map_err(|_| FfiError::NulInCommand(cmd.to_string()))?;
        let rc = unsafe { ngSpice_Command(c_cmd.as_ptr() as *mut c_char) };
        if rc != 0 {
            return Err(FfiError::CommandFailed(cmd.to_string(), rc));
        }
        Ok(())
    }

    /// Takes and clears everything captured by `send_char` so far (e.g. the
    /// text output of a `showmod` command).
    pub fn drain_output(&self) -> Vec<String> {
        std::mem::take(
            &mut *output_buffer()
                .lock()
                .expect("ngspice output buffer mutex poisoned"),
        )
    }

    /// Reads back the first real value of a vector previously created with
    /// e.g. `let x = @<instance>[<param>]`.
    pub fn get_real(&self, vecname: &str) -> Result<f64, FfiError> {
        let c_name =
            CString::new(vecname).map_err(|_| FfiError::NulInCommand(vecname.to_string()))?;
        let info = unsafe { ngGet_Vec_Info(c_name.as_ptr() as *mut c_char) };
        if info.is_null() {
            return Err(FfiError::VectorUnavailable(vecname.to_string()));
        }
        let info = unsafe { &*info };
        if info.v_realdata.is_null() || info.v_length == 0 {
            return Err(FfiError::VectorUnavailable(vecname.to_string()));
        }
        Ok(unsafe { *info.v_realdata })
    }
}
