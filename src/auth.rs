use std::ffi::{CStr, CString, OsStr};

use libc::c_char;

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
