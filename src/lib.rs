mod auth;
mod config;
mod desktop;
mod user;
mod xlib;

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command};

use anyhow::{anyhow, Result};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Password, Select};

pub use self::config::{Config, LaunchType};
use self::desktop::Desktop;
use self::user::User;
use self::xlib::XDisplay;
use crate::desktop::Environment;

pub fn login(config: Config) -> Result<()> {
    let user = select_user(&config)?;
    let desktop = select_desktop(&user, &config)?;
    let env = define_env(&user, &desktop)?;

    xorg(user, desktop, env, config)?;

    Ok(())
}

struct Env {
    runtime_dir: String,
    variables: HashMap<&'static str, OsString>,
}

fn define_env(user: &User, desktop: &Desktop) -> Result<Env> {
    let runtime_dir = format!("/run/user/{}", user.uid());

    let mut env: HashMap<&'static str, OsString> = HashMap::new();

    env.insert("HOME", user.home_dir().into());
    env.insert("PWD", user.home_dir().into());
    env.insert("USER", user.name().into());
    env.insert("LOGNAME", user.name().into());
    env.insert("XDG_CONFIG_HOME", user.home_dir().join("/.config").into());
    env.insert("XDG_RUNTIME_DIR", runtime_dir.clone().into());
    env.insert("XDG_SEAT", "seat0".into());
    env.insert("XDG_SESSION_CLASS", "user".into());
    env.insert("SHELL", user.shell().into());
    env.insert("LANG", "en_US.UTF-8".into());
    env.insert("PATH", std::env::var("PATH")?.into());

    env.insert("DESKTOP_SESSION", desktop.name.clone().into());
    env.insert("XDG_SESSION_DESKTOP", desktop.name.clone().into());

    {
        let runtime_dir = std::path::Path::new(&runtime_dir);
        if !runtime_dir.exists() {
            std::fs::create_dir_all(&runtime_dir)?;
            user::set_file_owner(&runtime_dir, user)?;
        }
    }

    std::env::set_current_dir(user.home_dir());

    Ok(Env {
        runtime_dir,
        variables: env,
    })
}

pub fn select_user(_config: &Config) -> Result<User> {
    let users = unsafe { user::all_human_users() }.collect::<Vec<_>>();
    if users.is_empty() {
        return Err(anyhow!("No users found"));
    }

    let theme = ColorfulTheme::default();
    let mut selection = Select::with_theme(&theme);
    for user in &users {
        selection.item(user.name().to_string_lossy());
    }
    let user = selection.default(0).with_prompt("Select user:").interact()?;

    loop {
        let password = Password::with_theme(&theme).with_prompt("Enter password:").interact()?;

        if auth::auth_password(users[user].name(), &password) {
            break Ok(users[user].clone());
        } else {
            println!("Invalid password!");
        }
    }
}

pub fn select_desktop(user: &User, config: &Config) -> Result<Desktop> {
    let desktops = desktop::all_desktops();
    if desktops.is_empty() {
        return Err(anyhow!("No desktops found"));
    }

    let last_desktop = desktop::get_last_desktop(user, &desktops);
    if let Some(auto_login_session) = &config.auto_login_session {
        let auto_login_session = auto_login_session.trim();
        if let Some(desktop) = desktops
            .iter()
            .find(|desktop| desktop.exec.starts_with(auto_login_session))
        {
            update_last_session(user, last_desktop, desktop);
            return Ok(desktop.clone());
        };
    }

    let theme = ColorfulTheme::default();
    let mut selection = Select::with_theme(&theme);
    for desktop in &desktops {
        selection.item(&desktop.name);
    }

    let selection = selection.default(0).with_prompt("Select desktop:").interact()?;
    Ok(desktops[selection].clone())
}

fn update_last_session(user: &User, last_desktop: Option<&Desktop>, desktop: &Desktop) {
    let last_session_changed = match last_desktop {
        Some(last_desktop) => last_desktop.exec != desktop.exec || last_desktop.env != desktop.env,
        None => true,
    };
    if last_session_changed {
        if let Err(e) = desktop::set_last_session(user, desktop) {
            eprintln!("Failed to set last session: {}", e);
        }
    }
}

fn xorg(user: User, desktop: Desktop, mut env: Env, config: Config) -> Result<()> {
    let free_display = get_free_xdisplay().ok_or_else(|| anyhow!("There is no free xdisplay"))?;

    let xauthority = Path::new(&env.runtime_dir).join(".dsdmona-xauth");
    let display = format!(":{}", free_display);

    env.variables.insert("XDG_SESSION_TYPE", "x11".into());
    env.variables.insert("XAUTHORITY", xauthority.as_os_str().to_owned());
    env.variables.insert("DISPLAY", display.clone().into());

    std::fs::remove_file(&xauthority);

    // Generate mcookie
    let output = exec_cmd("/usr/bin/mcookie", &user, &env).output()?;
    let mcookie = String::from_utf8(output.stdout.clone())?;

    // Generate xauth
    let _output = exec_cmd("/usr/bin/xauth", &user, &env)
        .arg("add")
        .arg("DISPLAY")
        .arg(".")
        .arg(mcookie.trim())
        .output()?;

    println!("Generated XAuthority");

    // Start X

    let mut xorg = Command::new("/usr/bin/Xorg")
        .arg(format!("vt{}", config.tty))
        .arg(&display)
        .envs(std::env::vars())
        .spawn()?;
    println!("Started Xorg");

    let start_xinit = || -> Result<(XDisplay, Child)> {
        let display = XDisplay::open(display)?;

        // Start xinit
        let xinit = prepare_gui_command(&user, &desktop, &env, &config)?.spawn()?;
        println!("Started XInit");

        Ok((display, xinit))
    };
    let (xdisplay, mut xinit) = match start_xinit() {
        Ok(r) => r,
        Err(e) => {
            unsafe { libc::kill(xorg.id() as i32, libc::SIGINT) };
            return Err(e);
        }
    };

    xinit.wait()?;
    println!("XInit finished");

    std::mem::drop(xdisplay);

    unsafe { libc::kill(xorg.id() as i32, libc::SIGINT) };
    xorg.wait()?;
    println!("XOrg finished");

    // Remove auth
    std::fs::remove_file(&xauthority);

    Ok(())
}

fn prepare_gui_command(user: &User, desktop: &Desktop, env: &Env, config: &Config) -> Result<Command> {
    let mut exec = desktop
        .exec
        .split(' ')
        .map(|item| OsString::from(item))
        .collect::<Vec<_>>();

    let exec: Vec<OsString> = if desktop.env == Environment::Xorg
        && config.launch_type == LaunchType::XInitRc
        && !exec.contains(&OsString::from(".xinitrc"))
        && user.home_dir().join("/.xinitrc").exists()
    {
        let mut result = vec!["/bin/sh".into(), user.home_dir().join("/.xinitrc").into_os_string()];
        result.extend(exec.into_iter());
        result
    } else if config.launch_type == LaunchType::DBus {
        let mut result = vec!["dbus-launch ".into()];
        result.extend(exec.into_iter());
        result
    } else {
        exec
    };

    if exec.is_empty() {
        Err(anyhow!("Invalid exec command"))
    } else {
        let mut command = exec_cmd(&exec[0], user, env);
        for arg in exec {
            command.arg(arg);
        }
        Ok(command)
    }
}

fn exec_cmd<T>(path: T, user: &User, env: &Env) -> Command
where
    T: AsRef<OsStr>,
{
    let mut command = Command::new(path);

    command.uid(user.uid()).gid(user.primary_group());
    for (key, value) in &env.variables {
        command.env(key, value);
    }
    command
}

fn get_free_xdisplay() -> Option<u32> {
    for i in 0..32 {
        let lock = format!("/tmp/.X{}-lock", i);
        if !Path::new(&lock).exists() {
            return Some(i);
        }
    }
    None
}
