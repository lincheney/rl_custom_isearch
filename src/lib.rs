#[macro_use]
extern crate lazy_static;
extern crate libc;

use std::io::{Write, Read};
use std::process::{Command, Stdio};

macro_rules! dynlib_call {
    ($func:ident($($args:expr),*)) => {{
        let ptr = {
            use ::libc::$func;
            $func($($args),*)
        };
        if ptr.is_null() {
            let error = ::libc::dlerror();
            if error.is_null() {
                Err(concat!("unknown error calling: ", stringify!($func)))
            } else {
                Err(::std::ffi::CStr::from_ptr(error).to_str().unwrap())
            }
        } else {
            Ok(ptr)
        }
    }}
}

macro_rules! dlopen {
    ($name:expr) => { dlopen!($name, ::libc::RTLD_LAZY) };
    ($name:expr, $flags:expr) => { dynlib_call!(dlopen($name.as_ptr() as _, $flags)) };
}

macro_rules! dlsym {
    ($handle:expr, $name:expr) => {
        dlsym!($handle, $name, _)
    };
    ($handle:expr, $name:expr, $type:ty) => {{
        let name = concat!($name, "\0");
        #[allow(clippy::transmute_ptr_to_ptr)]
        dynlib_call!(dlsym($handle, name.as_ptr() as _)).map(|sym|
            std::mem::transmute::<_, $type>(sym)
        )
    }}
}

struct CArray {
    ptr: *const *const readline::HistEntry
}

impl CArray {
    fn new(ptr: *const *const readline::HistEntry) -> Self {
        CArray{ptr}
    }
}

impl Iterator for CArray {
    type Item = &'static readline::HistEntry;
    fn next(&mut self) -> Option<&'static readline::HistEntry> {
        if self.ptr.is_null() { return None }
        if unsafe{ *(self.ptr) }.is_null() { return None }
        let value = unsafe{ &**self.ptr };
        self.ptr = unsafe{ self.ptr.offset(1) };
        Some(value)
    }
}

mod readline {
    use std::ffi::CStr;
    pub use self::lib::{HistEntry, rl_reverse_search_history};

    pub fn get_history() -> *const *const HistEntry {
        unsafe{ lib::history_list() }
    }

    pub fn refresh_line() {
        unsafe{ lib::rl_refresh_line(0, 0) };
    }

    pub fn set_text(buf: Vec<u8>) {
        let ptr = buf.as_ptr();
        unsafe {
            // clear line
            lib::rl_end_of_line(0, 0);
            lib::rl_unix_line_discard(0, 0);
            lib::rl_refresh_line(0, 0);
            // insert selected
            lib::rl_insert_text(ptr as _);
        }
    }

    pub fn get_readline_name() -> Option<&'static str> {
        unsafe {
            let name = (*lib::rl_readline_name.ptr()).ptr();
            if name.is_null() {
                None
            } else {
                CStr::from_ptr(name).to_str().ok()
            }
        }
    }

    #[allow(non_upper_case_globals)]
    mod lib {
        use std::marker::PhantomData;
        use std::ffi::CStr;

        #[repr(C)]
        pub struct HistEntry {
            line: *const i8,
            timestamp: *const i8,
            data: *const i8,
        }

        impl HistEntry {
            pub fn get_line(&self) -> &[u8] {
                if self.line.is_null() { return &[0;0]; }
                unsafe{ CStr::from_ptr(self.line) }.to_bytes()
            }
        }

        pub struct Pointer<T>(usize, PhantomData<T>);
        impl<T> Pointer<T> {
            pub fn new(ptr: *mut T)    -> Self { Self(ptr as _, PhantomData) }
            pub fn ptr(&self)        -> *mut T { self.0 as *mut T }
            pub unsafe fn set(&self, value: T) { *self.ptr() = value; }
        }

        lazy_static! {
            pub static ref libreadline: Pointer<libc::c_void> = Pointer::new(unsafe {
                if dlsym!(libc::RTLD_DEFAULT, "rl_initialize", usize).is_ok() {
                    libc::RTLD_DEFAULT
                } else {
                    dlopen!(b"libreadline.so\0").unwrap()
                }
            });
        }
        macro_rules! readline_lookup {
            ($name:ident: $type:ty) => {
                lazy_static! { pub static ref $name: $type = unsafe{ dlsym!(libreadline.ptr(), stringify!($name)) }.unwrap(); }
            }
        }

        readline_lookup!(history_list:              unsafe extern fn() -> *const *const HistEntry);
        readline_lookup!(rl_unix_line_discard:      unsafe extern fn(isize, isize) -> isize);
        readline_lookup!(rl_refresh_line:           unsafe extern fn(isize, isize) -> isize);
        readline_lookup!(rl_end_of_line:            unsafe extern fn(isize, isize) -> isize);
        readline_lookup!(rl_reverse_search_history: unsafe extern fn(isize, isize) -> isize);
        readline_lookup!(rl_insert_text:            unsafe extern fn(*const i8) -> isize);
        readline_lookup!(rl_readline_name:          Pointer<Pointer<i8>>);
    }
}

#[no_mangle]
pub extern fn rl_custom_function(direction: isize, key: isize) -> isize {
    if custom_isearch() { return 0; }
    unsafe{ readline::rl_reverse_search_history(direction, key) }
}

fn custom_isearch() -> bool {
    let mut command = Command::new("rl_custom_isearch");
    command.stdin(Stdio::piped()).stdout(Stdio::piped());

    // pass the readline name to process
    if let Some(name) = readline::get_readline_name() {
        command.env("READLINE_NAME", name);
    }

    let mut process = match command.spawn() {
        Ok(process) => process,
        // failed to run, do default readline search
        Err(_) => { return false },
    };
    let mut stdin = process.stdin.unwrap();

    for entry in CArray::new(readline::get_history()) {
        let line = entry.get_line();
        // break on errors (but otherwise ignore)
        if ! ( stdin.write_all(line).is_ok() && stdin.write_all(b"\n").is_ok() ) {
            break
        }
    }

    // pass back stdin for process to close
    process.stdin = Some(stdin);
    match process.wait() {
        // failed to run, do default readline search
        Err(_) => {
            false
        },
        // exited with code != 0, leave line as is
        Ok(code) if ! code.success() => {
            readline::refresh_line();
            true
        },
        Ok(_) => {
            let mut stdout = process.stdout.unwrap();
            let mut buf: Vec<u8> = vec![];
            if stdout.read_to_end(&mut buf).is_err() {
                // failed to read stdout, default to readline search
                return false
            }
            // make sure buf is null terminated
            if *buf.last().unwrap_or(&1) != 0 { buf.push(0); }

            readline::set_text(buf);
            true
        }
    }
}
