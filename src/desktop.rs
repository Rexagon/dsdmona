use std::fs::File;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::str::FromStr;

use anyhow::{anyhow, Result};
use libc::uid_t;
use walkdir::WalkDir;

use crate::user::{self, User};

const XORG_SESSIONS_PATH: &str = "/usr/share/xsessions";
const LAST_SESSION_PATH: &str = ".cache/dsdmona/last_session";

#[derive(Debug, Clone)]
pub struct Desktop {
    pub name: String,
    pub comment: String,
    pub exec: String,
    pub env: Environment,
}

pub fn all_desktops() -> Vec<Desktop> {
    Environment::Xorg.list_desktops()
}

pub fn get_last_desktop<'a>(user: &User, desktops: &'a [Desktop]) -> Option<&'a Desktop> {
    let last_session = get_last_session(user)?;
    desktops
        .iter()
        .find(|desktop| desktop.env == last_session.env && desktop.exec == last_session.exec)
}

pub struct LastSession {
    pub uid: uid_t,
    pub exec: String,
    pub env: Environment,
}

pub fn get_last_session(user: &User) -> Option<LastSession> {
    let path = user.home_dir().join(LAST_SESSION_PATH);
    if !path.exists() {
        return None;
    }

    let last_session = std::fs::read_to_string(path).ok()?;
    let last_session = last_session.trim();

    let split = last_session.find(';')?;
    let (exec, env) = last_session.split_at(split);
    let env = match Environment::from_str(env) {
        Ok(env) => env,
        Err(e) => {
            eprintln!("Failed to parse last session: {}", e);
            return None;
        }
    };

    Some(LastSession {
        uid: user.uid(),
        exec: exec.to_owned(),
        env,
    })
}

pub fn set_last_session(usr: &User, desktop: &Desktop) -> Result<()> {
    let previous_user = user::get_current();

    user::set_fs_user(usr);

    let path = usr.home_dir().join(LAST_SESSION_PATH);
    std::fs::create_dir_all(&path)?;
    std::fs::write(&path, format!("{};{}\n", desktop.exec, desktop.env));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o744))?;

    user::set_fs_user(&previous_user);
    Ok(())
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Environment {
    Xorg,
}

impl Environment {
    pub fn list_desktops(&self) -> Vec<Desktop> {
        match self {
            Environment::Xorg => WalkDir::new(XORG_SESSIONS_PATH)
                .into_iter()
                .filter_map(|e| match e {
                    Ok(entry) if entry.file_type().is_file() => match entry.path().extension()?.to_str()? {
                        "desktop" => self.load_desktop(entry.path()).ok(),
                        _ => None,
                    },
                    _ => None,
                })
                .collect(),
        }
    }

    fn load_desktop<P>(&self, path: &P) -> Result<Desktop>
    where
        P: AsRef<Path> + ?Sized,
    {
        match self {
            Environment::Xorg => {
                let file = File::open(path)?;
                let reader = BufReader::new(file);

                let mut desktop = Desktop {
                    name: Default::default(),
                    comment: Default::default(),
                    exec: Default::default(),
                    env: Environment::Xorg,
                };

                let mut is_application = false;

                let mut desktop_entry_description = false;
                for item in reader.lines() {
                    let line = item?;
                    let line = line.trim();

                    if line.starts_with('#') {
                        continue;
                    } else if line.starts_with("[") {
                        desktop_entry_description = line == "[Desktop Entry]";
                    } else if desktop_entry_description {
                        if let Some(split) = line.find('=') {
                            let (property, value) = line.split_at(split);
                            let value = match value.find('#') {
                                Some(split) => &value[1..split],
                                None => &value[1..],
                            };

                            match property.to_lowercase().as_ref() {
                                "type" => is_application = value == "Application",
                                "name" => desktop.name = value.to_owned(),
                                "comment" => desktop.comment = value.to_owned(),
                                "exec" => desktop.exec = value.to_owned(),
                                _ => {}
                            }
                        }
                    }
                }

                if !is_application {
                    return Err(anyhow!("Invalid desktop entry. Type is not an `Application`"));
                }

                Ok(desktop)
            }
        }
    }
}

impl std::fmt::Display for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Environment::Xorg => f.write_str("Xorg"),
        }
    }
}

impl FromStr for Environment {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Xorg" | "xorg" => Ok(Environment::Xorg),
            _ => Err(anyhow!("Unknown environment")),
        }
    }
}
