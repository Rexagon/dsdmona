use std::fs::File;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::Result;
use libc::uid_t;
use walkdir::WalkDir;

use crate::user::User;

const XORG_SESSIONS_PATH: &str = "/usr/share/xsessions";
const LAST_SESSION_PATH: &str = ".cache/dsdmona/last_session";

#[derive(Debug, Clone)]
pub struct Desktop {
    pub name: String,
    pub comment: String,
    pub exec: String,
}

impl Desktop {
    pub fn all() -> Vec<Self> {
        WalkDir::new(XORG_SESSIONS_PATH)
            .into_iter()
            .filter_map(|e| match e {
                Ok(entry) if entry.file_type().is_file() => match entry.path().extension()?.to_str()? {
                    "desktop" => Self::load(entry.path()).ok(),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);

        let mut desktop = Desktop {
            name: Default::default(),
            comment: Default::default(),
            exec: Default::default(),
        };

        let mut is_application = false;

        let mut desktop_entry_description = false;
        for item in reader.lines() {
            let line = item?;
            let line = line.trim();

            if line.starts_with('#') {
                continue;
            } else if line.starts_with('[') {
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

        anyhow::ensure!(is_application, "Invalid desktop entry. Type is not an `Application`");
        Ok(desktop)
    }
}

pub struct LastSession {
    pub uid: uid_t,
    pub exec: String,
}

impl User {
    pub fn get_last_desktop<'a>(&self, desktops: &'a [Desktop]) -> Option<&'a Desktop> {
        let last_session = self.get_last_session()?;
        desktops.iter().find(|desktop| desktop.exec == last_session.exec)
    }

    pub fn get_last_session(&self) -> Option<LastSession> {
        let path = self.home_dir().join(LAST_SESSION_PATH);
        if !path.exists() {
            return None;
        }

        let last_session = std::fs::read_to_string(path).ok()?;
        let exec = last_session.trim();

        Some(LastSession {
            uid: self.uid(),
            exec: exec.to_owned(),
        })
    }

    pub fn set_last_session(&self, desktop: &Desktop) -> Result<()> {
        self.use_for_fs(|| {
            let path = self.home_dir().join(LAST_SESSION_PATH);
            std::fs::create_dir_all(&path)?;
            std::fs::write(&path, &desktop.exec)?;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o744))?;
            Ok(())
        })
    }
}
