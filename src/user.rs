use std::ffi::{CStr, CString, OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use libc::passwd as c_passwd;
use libc::spwd as c_spwd;
use libc::{c_char, gid_t, uid_t};
use zeroize::Zeroizing;

pub const MIN_UID: uid_t = 1000;
pub const MAX_UID: uid_t = 65534;

#[derive(Clone)]
pub struct User {
    uid: uid_t,
    primary_group: gid_t,
    name: Arc<OsStr>,
    home_dir: PathBuf,
    shell: PathBuf,
}

impl User {
    pub fn current() -> Self {
        unsafe { Self::new(libc::getuid()).unwrap() }
    }

    pub fn all() -> Vec<Self> {
        let iter = unsafe { AllUsers::new() };
        iter.filter(|user| user.uid >= MIN_UID && user.uid < MAX_UID).collect()
    }

    pub fn new(uid: uid_t) -> Result<Self> {
        let mut passwd = unsafe { std::mem::zeroed::<c_passwd>() };
        let mut buf = vec![0; 2048];
        let mut result = std::ptr::null_mut::<c_passwd>();

        loop {
            let r = unsafe { libc::getpwuid_r(uid, &mut passwd, buf.as_mut_ptr(), buf.len(), &mut result) };

            if r != libc::ERANGE {
                break;
            }

            let newsize = buf.len() * 2;
            buf.resize(newsize, 0);
        }

        anyhow::ensure!(result == &mut passwd, "User not found");

        Ok(unsafe { cpasswd_to_user(result.read()) })
    }

    pub fn uid(&self) -> uid_t {
        self.uid
    }

    pub fn primary_group(&self) -> gid_t {
        self.primary_group
    }

    pub fn name(&self) -> &OsStr {
        self.name.as_ref()
    }

    pub fn home_dir(&self) -> &Path {
        Path::new(&self.home_dir)
    }

    pub fn shell(&self) -> &Path {
        Path::new(&self.shell)
    }

    pub fn use_for_fs<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        unsafe fn set_fs_user(uid: uid_t, group: gid_t) {
            libc::setfsuid(uid);
            libc::setgid(group);
        }

        unsafe {
            // Get current user group
            let current_user = libc::getpwuid(libc::getuid());
            assert!(!current_user.is_null());
            let current_uid = (*current_user).pw_uid;
            let current_group = (*current_user).pw_gid;

            // Set user as fs owner
            set_fs_user(self.uid, self.primary_group);
            let res = f();
            set_fs_user(current_uid, current_group);

            // Done
            res
        }
    }

    pub fn set_file_owner<T: AsRef<Path>>(&self, path: T) -> Result<()> {
        let path = CString::new(path.as_ref().as_os_str().as_bytes())?;
        let r = unsafe { libc::chown(path.as_ptr(), self.uid, self.primary_group) };
        anyhow::ensure!(r == 0, "Failed to chown file (errno: {})", r);

        Ok(())
    }

    pub fn check_password(&self, password: &str) -> Result<bool> {
        #[link(name = "crypt")]
        extern "C" {
            fn crypt(key: *const c_char, salt: *const c_char) -> *mut c_char;
        }

        let password: Zeroizing<_> = CString::new(password).map(Zeroizing::new).unwrap();
        let user_password: Zeroizing<_> = self.get_password()?;

        Ok(unsafe {
            let result = crypt(password.as_ptr(), user_password.as_ptr());
            !result.is_null() && user_password.as_ref() == CStr::from_ptr(result)
        })
    }

    pub fn get_password(&self) -> Result<Zeroizing<CString>> {
        let name = CString::new(self.name.as_bytes())?;

        let mut spwd = unsafe { std::mem::zeroed::<c_spwd>() };
        let mut buf = Zeroizing::new(vec![0; 2048]);
        let mut result = std::ptr::null_mut::<c_spwd>();

        loop {
            let r = unsafe { libc::getspnam_r(name.as_ptr(), &mut spwd, buf.as_mut_ptr(), buf.len(), &mut result) };

            match r {
                libc::EACCES => {
                    anyhow::bail!("The caller does not have permission to access the shadow password file")
                }
                libc::ERANGE => {
                    let newsize = buf
                        .len()
                        .checked_mul(2)
                        .ok_or_else(|| anyhow::Error::msg("Failed to increase spwd buffer"))?;
                    buf.resize(newsize, 0);
                }
                _ => break,
            }
        }

        anyhow::ensure!(
            result == &mut spwd,
            "Failed to find a shadow file record for the specified username"
        );

        let bytes = unsafe { CStr::from_ptr(spwd.sp_pwdp).to_bytes() };
        let result = Zeroizing::new(CString::new(bytes)?);

        Ok(result)
    }
}

struct AllUsers;

impl AllUsers {
    unsafe fn new() -> Self {
        libc::setpwent();
        Self
    }
}

impl Drop for AllUsers {
    fn drop(&mut self) {
        unsafe { libc::endpwent() };
    }
}

impl Iterator for AllUsers {
    type Item = User;

    fn next(&mut self) -> Option<Self::Item> {
        let result = unsafe { libc::getpwent() };
        if result.is_null() {
            None
        } else {
            let user = unsafe { cpasswd_to_user(result.read()) };
            Some(user)
        }
    }
}

unsafe fn cpasswd_to_user(passwd: c_passwd) -> User {
    unsafe fn from_raw_buf<'a, T>(p: *const c_char) -> T
    where
        T: From<&'a OsStr>,
    {
        T::from(OsStr::from_bytes(CStr::from_ptr(p).to_bytes()))
    }

    let name = from_raw_buf(passwd.pw_name);
    let home_dir = from_raw_buf::<OsString>(passwd.pw_dir).into();
    let shell = from_raw_buf::<OsString>(passwd.pw_shell).into();
    User {
        uid: passwd.pw_uid,
        primary_group: passwd.pw_gid,
        name,
        home_dir,
        shell,
    }
}
