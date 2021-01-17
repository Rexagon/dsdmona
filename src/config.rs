#[derive(Debug, Clone)]
pub struct Config {
    pub tty: u8,
    pub launch_type: LaunchType,
    pub auto_login_session: Option<String>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum LaunchType {
    XInitRc,
    DBus,
}
