use std::ffi::{CStr, CString, OsStr};

use libc::utmpx as c_utmpx;
use libc::{c_char, pid_t};

use crate::user;

#[link(name = "crypt")]
extern "C" {
    fn crypt(key: *const c_char, salt: *const c_char) -> *mut c_char;
}

pub fn auth_password<T>(username: &T, password: &str) -> bool
where
    T: AsRef<OsStr> + ?Sized,
{
    let password = CString::new(password).unwrap();
    let user_password = match user::get_user_password(username) {
        Ok(password) => password,
        Err(e) => {
            eprintln!("Failed to authorize: {}", e);
            return false;
        }
    };

    unsafe {
        let result = crypt(password.as_ptr(), user_password.as_ptr());
        if result.is_null() {
            return false;
        }
        user_password.as_ref() == CStr::from_ptr(result)
    }
}

fn prepare_utml_entry(username: &str, pid: pid_t, tty_no: &str) -> c_utmpx {
    let mut utmpx = unsafe { std::mem::zeroed::<c_utmpx>() };

    let dev_name = unsafe { CStr::from_ptr(libc::ttyname(libc::STDIN_FILENO)) }
        .to_str()
        .unwrap();
    if dev_name.len() <= 8 || !matches!(&dev_name[5..9], "tty" | "pty") {
        panic!("Unsupported terminal");
    }
    let tty = &dev_name[5..];

    utmpx.ut_type = libc::USER_PROCESS;
    utmpx.ut_pid = pid;

    for i in 0..(tty.len() - 3) {
        utmpx.ut_id[0] = tty.as_bytes()[3 + i] as i8;
    }
    for i in 0..tty.len() {
        utmpx.ut_line[i] = tty.as_bytes()[i] as i8;
    }

    todo!()
}
