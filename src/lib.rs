#[macro_use]
extern crate lazy_static;
extern crate libc;

use std::io::{Write, Read};
use std::process::{Command, Stdio};

type DynlibResult<T> = Result<T, &'static str>;

macro_rules! dump_error {
    ($result:expr, $default:expr) => {
        match $result {
            Ok(x) => x,
            Err(e) => { eprintln!("{}", e); return $default },
        }
    }
}

macro_rules! dynlib_call {
    ($func:ident($($args:expr),*)) => {{
        let ptr = libc::$func($($args),*);
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

    pub fn get_history() -> ::DynlibResult<*const *const HistEntry> {
        Ok(unsafe{ (*lib::history_list)?() })
    }

    pub fn refresh_line() -> ::DynlibResult<()> {
        unsafe{ (*lib::rl_refresh_line)?(0, 0) };
        Ok(())
    }

    pub fn set_text(buf: Vec<u8>) -> ::DynlibResult<()> {
        unsafe {
            // clear line
            (*lib::rl_end_of_line)?(0, 0);
            (*lib::rl_unix_line_discard)?(0, 0);
            (*lib::rl_refresh_line)?(0, 0);
            // insert selected
            (*lib::rl_insert_text)?(buf.as_ptr() as _);
        }
        Ok(())
    }

    pub fn get_readline_name() -> ::DynlibResult<Option<&'static str>> {
        match lib::rl_readline_name.as_ref() {
            Ok(n) if n.ptr().is_null() => Ok(None),
            Ok(n) => Ok(unsafe{ CStr::from_ptr(n.ptr() as _) }.to_str().ok()),
            Err(e) => Err(e)
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
            pub fn ptr(&self) -> *mut T { self.0 as *mut T }
        }

        macro_rules! readline_lookup {
            ($name:ident: $type:ty) => {
                readline_lookup!($name: $type; libc::RTLD_DEFAULT);
            };
            ($name:ident: $type:ty; $handle:expr) => {
                lazy_static! {
                    pub static ref $name: ::DynlibResult<$type> = unsafe {
                        dlsym!($handle, stringify!($name)).or_else(|_|
                            dynlib_call!(dlopen(b"libreadline.so\0".as_ptr() as _, libc::RTLD_NOLOAD | libc::RTLD_LAZY))
                            .and_then(|lib| dlsym!(lib, stringify!($name)))
                        )};
                }
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
    if dump_error!(custom_isearch(), 0) { return 0 }
    let func = dump_error!(*readline::rl_reverse_search_history, 0);
    unsafe{ func(direction, key) }
}

fn custom_isearch() -> DynlibResult<bool> {
    let mut command = Command::new("rl_custom_isearch");
    command.stdin(Stdio::piped()).stdout(Stdio::piped());

    // pass the readline name to process
    if let Some(name) = readline::get_readline_name()? {
        command.env("READLINE_NAME", name);
    }

    let mut process = match command.spawn() {
        Ok(process) => process,
        // failed to run, do default readline search
        Err(_) => { return Ok(false) },
    };
    let mut stdin = process.stdin.unwrap();

    for entry in CArray::new(readline::get_history()?) {
        let line = entry.get_line();
        // break on errors (but otherwise ignore)
        if ! ( stdin.write_all(line).is_ok() && stdin.write_all(b"\n").is_ok() ) {
            break
        }
    }

    // pass back stdin for process to close
    process.stdin = Some(stdin);
    Ok(match process.wait() {
        // failed to run, do default readline search
        Err(_) => {
            false
        },
        // exited with code != 0, leave line as is
        Ok(code) if ! code.success() => {
            readline::refresh_line()?;
            true
        },
        Ok(_) => {
            let mut stdout = process.stdout.unwrap();
            let mut buf: Vec<u8> = vec![];
            if stdout.read_to_end(&mut buf).is_err() {
                // failed to read stdout, default to readline search
                return Ok(false)
            }
            // make sure buf is null terminated
            if *buf.last().unwrap_or(&1) != 0 { buf.push(0); }

            readline::set_text(buf)?;
            true
        }
    })
}
