use std::str::FromStr;

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

impl FromStr for LaunchType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "xinitrc" => Ok(Self::XInitRc),
            "dbus" => Ok(Self::DBus),
            _ => anyhow::bail!("Unknown launch type"),
        }
    }
}
