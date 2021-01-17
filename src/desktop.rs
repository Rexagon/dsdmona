use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::{anyhow, Result};
use walkdir::WalkDir;

use crate::user::User;

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

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Environment {
    Xorg,
}

impl Environment {
    pub fn list_desktops(&self) -> Vec<Desktop> {
        match self {
            Environment::Xorg => WalkDir::new("/usr/share/xsessions")
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
