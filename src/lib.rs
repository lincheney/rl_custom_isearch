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

    pub fn history_each<F>(mut callback: F) where F: FnMut(&[u8]) {
        let mut history = unsafe{ history_list() };
        if history.is_null() { return; }

        while ! unsafe{ (*history) }.is_null() {
            let entry = unsafe{ &**history };
            callback(entry.get_line());
            history = unsafe{ history.offset(1) };
        }
    }

    #[link(name = "readline")]
    extern {
        fn history_list() -> *const *const HistEntry;
        pub fn rl_unix_line_discard(count: isize, key: isize) -> isize;
        pub fn rl_refresh_line(count: isize, key: isize) -> isize;
        pub fn rl_end_of_line(count: isize, key: isize) -> isize;
        pub fn rl_insert_text(string: *const u8) -> isize;
    }

    fn get_original_fn(name: &str) -> fn(isize, isize)->isize {
        let ptr = name.as_ptr();
        let func = unsafe{ ::libc::dlsym(::libc::RTLD_NEXT, ptr as *const i8) };
        unsafe{ ::std::mem::transmute(func) }
    }

    lazy_static! {
        pub static ref RL_REVERSE_SEARCH_HISTORY: fn(isize, isize)->isize = get_original_fn("rl_reverse_search_history\0");
        pub static ref RL_FORWARD_SEARCH_HISTORY: fn(isize, isize)->isize = get_original_fn("rl_forward_search_history\0");
    }
}

#[no_mangle]
pub extern fn rl_reverse_search_history(direction: isize, key: isize) -> isize {
    // custom_isearch();
    readline::RL_REVERSE_SEARCH_HISTORY(direction, key)
}

fn custom_isearch() {
    let mut process = Command::new("fzf").arg("+m").arg("--tac").arg("--print0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn().expect("could not find fzf");
    let mut stdin = process.stdin.unwrap();

    readline::history_each(|line| {
        stdin.write_all(line).unwrap();
        stdin.write_all(b"\n").unwrap();
    });

    process.stdin = Some(stdin);
    if ! process.wait().unwrap().success() {
        unsafe{ readline::rl_refresh_line(0, 0); }
        return
    }

    let mut stdout = process.stdout.expect("could not open stdout");
    let mut buf: Vec<u8> = vec![];
    stdout.read_to_end(&mut buf).unwrap();
    // make sure buf is null terminated
    if *buf.last().unwrap_or(&1) != 0 { buf.push(0); }

    let buf = buf.as_ptr();
    unsafe {
        // clear line
        readline::rl_end_of_line(0, 0);
        readline::rl_unix_line_discard(0, 0);
        readline::rl_refresh_line(0, 0);
        // insert selected
        readline::rl_insert_text(buf);
    }
}
