#[macro_use]
extern crate lazy_static;
extern crate libc;

use std::io::{Write, Read};
use std::process::{Command, Stdio};

mod readline {
    use std::os::raw::c_char;
    use std::ffi::CStr;

    #[repr(C)]
    struct HistEntry {
        line: *const c_char,
        timestamp: *const c_char,
        data: *const c_char,
    }

    impl HistEntry {
        pub fn get_line<'a>(&'a self) -> &'a [u8] {
            if self.line.is_null() { return &[0;0]; }
            unsafe{ CStr::from_ptr((&self).line) }.to_bytes()
        }
    }

    // run callback for each history or until callback returns false
    pub fn history_each<F>(mut callback: F) where F: FnMut(&[u8])->bool {
        let mut history = unsafe{ history_list() };
        if history.is_null() { return; }

        while ! unsafe{ (*history) }.is_null() {
            let entry = unsafe{ &**history };
            if ! callback(entry.get_line()) { return }
            history = unsafe{ history.offset(1) };
        }
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

    #[link(name = "readline")]
    extern {
        fn history_list() -> *const *const HistEntry;
        fn rl_unix_line_discard(count: isize, key: isize) -> isize;
        fn rl_refresh_line(count: isize, key: isize) -> isize;
        fn rl_end_of_line(count: isize, key: isize) -> isize;
        fn rl_insert_text(string: *const u8) -> isize;
    }

    // look up fn via dlsym
    fn get_original_fn(name: &str) -> unsafe fn(isize, isize)->isize {
        let ptr = name.as_ptr();
        let func = unsafe{ ::libc::dlsym(::libc::RTLD_NEXT, ptr as *const i8) };
        unsafe{ ::std::mem::transmute(func) }
    }

    lazy_static! {
        pub static ref RL_REVERSE_SEARCH_HISTORY: unsafe fn(isize, isize)->isize = get_original_fn("rl_reverse_search_history\0");
        pub static ref RL_FORWARD_SEARCH_HISTORY: unsafe fn(isize, isize)->isize = get_original_fn("rl_forward_search_history\0");
    }
}

#[no_mangle]
pub extern fn rl_reverse_search_history(direction: isize, key: isize) -> isize {
    if custom_isearch() { return 0; }
    unsafe{ readline::RL_REVERSE_SEARCH_HISTORY(direction, key) }
}

#[no_mangle]
pub extern fn rl_forward_search_history(direction: isize, key: isize) -> isize {
    if custom_isearch() { return 0; }
    unsafe{ readline::RL_FORWARD_SEARCH_HISTORY(direction, key) }
}

fn custom_isearch() -> bool {
    let mut process = match Command::new("rl_custom_isearch")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn() {
            Ok(process) => process,
            // failed to run, do default readline search
            Err(_) => { return false },
        };
    let mut stdin = process.stdin.unwrap();

    readline::history_each(|line| {
        // break on errors (but otherwise ignore)
        stdin.write_all(line).is_ok()
            && stdin.write_all(b"\n").is_ok()
    });

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
