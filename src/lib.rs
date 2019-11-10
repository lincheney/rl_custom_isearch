extern crate libc;

use std::io::{Write, Read};
use std::process::{Command, Stdio};

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
    use std::os::raw::c_char;
    use std::ffi::CStr;

    #[repr(C)]
    pub struct HistEntry {
        line: *const c_char,
        timestamp: *const c_char,
        data: *const c_char,
    }

    impl HistEntry {
        pub fn get_line(&self) -> &[u8] {
            if self.line.is_null() { return &[0;0]; }
            unsafe{ CStr::from_ptr(self.line) }.to_bytes()
        }
    }

    pub fn get_history() -> *const *const HistEntry {
        unsafe{ history_list() }
    }

    pub fn refresh_line() {
        unsafe{ rl_refresh_line(0, 0) };
    }

    pub fn set_text(buf: Vec<u8>) {
        let ptr = buf.as_ptr();
        unsafe {
            // clear line
            rl_end_of_line(0, 0);
            rl_unix_line_discard(0, 0);
            rl_refresh_line(0, 0);
            // insert selected
            rl_insert_text(ptr);
        }
    }

    pub fn get_readline_name() -> Option<&'static str> {
        if unsafe{ rl_readline_name }.is_null() { return None; }
        unsafe{ CStr::from_ptr(rl_readline_name) }.to_str().ok()
    }

    #[link(name = "readline")]
    extern {
        fn history_list() -> *const *const HistEntry;
        fn rl_unix_line_discard(count: isize, key: isize) -> isize;
        fn rl_refresh_line(count: isize, key: isize) -> isize;
        fn rl_end_of_line(count: isize, key: isize) -> isize;
        fn rl_insert_text(string: *const u8) -> isize;
        static rl_readline_name: *const c_char;

        pub fn rl_reverse_search_history(direction: isize, key: isize) -> isize;
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
