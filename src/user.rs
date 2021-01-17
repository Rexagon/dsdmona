use std::collections::HashMap;
use std::ffi::{CStr, CString, OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use libc::group as c_group;
use libc::passwd as c_passwd;
use libc::{c_char, c_int, gid_t, uid_t};

#[derive(Clone)]
pub struct User {
    uid: uid_t,
    primary_group: gid_t,
    name: Arc<OsStr>,
    home_dir: PathBuf,
    shell: PathBuf,
    password: OsString,
}

impl User {
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

    pub fn password(&self) -> &OsStr {
        &self.password
    }
}

#[derive(Clone)]
pub struct Group {
    gid: gid_t,
    name: Arc<OsStr>,
    members: Vec<OsString>,
}

impl Group {
    pub fn gid(&self) -> gid_t {
        self.gid
    }

    pub fn name(&self) -> &OsStr {
        self.name.as_ref()
    }

    pub fn members(&self) -> &[OsString] {
        &self.members
    }
}

unsafe fn from_raw_buf<'a, T>(p: *const c_char) -> T
where
    T: From<&'a OsStr>,
{
    T::from(OsStr::from_bytes(CStr::from_ptr(p).to_bytes()))
}

unsafe fn cpasswd_to_user(passwd: c_passwd) -> User {
    let name = from_raw_buf(passwd.pw_name);
    let home_dir = from_raw_buf::<OsString>(passwd.pw_dir).into();
    let shell = from_raw_buf::<OsString>(passwd.pw_shell).into();
    let password = from_raw_buf::<OsString>(passwd.pw_passwd);
    User {
        uid: passwd.pw_uid,
        primary_group: passwd.pw_gid,
        name,
        home_dir,
        shell,
        password,
    }
}

unsafe fn cgroup_to_group(group: c_group) -> Group {
    let name = from_raw_buf(group.gr_name);
    Group {
        gid: group.gr_gid,
        name,
        members: get_group_members(group.gr_mem),
    }
}

unsafe fn get_group_members(groups: *mut *mut c_char) -> Vec<OsString> {
    let mut members = Vec::new();

    for i in 0.. {
        let username = groups.offset(i);
        if username.is_null() || (*username).is_null() {
            break;
        } else {
            members.push(from_raw_buf(*username));
        }
    }

    members
}

unsafe fn get_user_by_uid(uid: uid_t) -> Option<User> {
    let mut passwd = unsafe { std::mem::zeroed::<c_passwd>() };
    let mut buf = vec![0; 2048];
    let mut result = std::ptr::null_mut::<c_passwd>();

    loop {
        let r = unsafe { libc::getpwuid_r(uid, &mut passwd, buf.as_mut_ptr(), buf.len(), &mut result) };

        if r != libc::ERANGE {
            break;
        }

        let newsize = buf.len().checked_mul(2)?;
        buf.resize(newsize, 0);
    }

    if result.is_null() {
        return None;
    }

    if result != &mut passwd {
        return None;
    }

    let user = unsafe { cpasswd_to_user(result.read()) };
    Some(user)
}

unsafe fn get_group_by_gid(gid: gid_t) -> Option<Group> {
    let mut passwd = unsafe { std::mem::zeroed::<c_group>() };
    let mut buf = vec![0; 2048];
    let mut result = std::ptr::null_mut::<c_group>();

    loop {
        let r = unsafe { libc::getgrgid_r(gid, &mut passwd, buf.as_mut_ptr(), buf.len(), &mut result) };

        if r != libc::ERANGE {
            break;
        }

        let newsize = buf.len().checked_mul(2)?;
        buf.resize(newsize, 0);
    }

    if result.is_null() {
        return None;
    }

    if result != &mut passwd {
        return None;
    }

    let group = unsafe { cgroup_to_group(result.read()) };
    Some(group)
}

unsafe fn get_user_groups<T>(username: &T, gid: gid_t) -> Option<Vec<Group>>
where
    T: AsRef<OsStr> + ?Sized,
{
    let mut buf: Vec<gid_t> = vec![0; 1024];
    let username = CString::new(username.as_ref().as_bytes()).unwrap();
    let mut count = buf.len() as c_int;

    let r = unsafe { libc::getgrouplist(username.as_ptr(), gid, buf.as_mut_ptr(), &mut count) };
    if r < 0 {
        return None;
    }

    buf.dedup();
    buf.into_iter()
        .filter_map(|i| get_group_by_gid(i))
        .collect::<Vec<_>>()
        .into()
}

pub unsafe fn all_users() -> impl Iterator<Item = User> {
    AllUsers::new()
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
