use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command};
use std::time::Duration;

use anyhow::Result;
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Password, Select};
use signal_hook::consts::signal::*;
use signal_hook::iterator::Signals;
use zeroize::Zeroizing;

pub use self::config::{Config, LaunchType};
use self::desktop::Desktop;
use self::user::User;
use self::xdisplay::XDisplay;

mod config;
mod desktop;
mod user;
mod xdisplay;

pub fn login(config: Config) -> Result<()> {
    let user = select_user(&config)?;
    let desktop = select_desktop(&user, &config)?;
    let env = Env::define(&user, &desktop)?;

    xorg(user, desktop, env, config)?;

    Ok(())
}

pub fn select_user(_config: &Config) -> Result<User> {
    let users = User::all();
    anyhow::ensure!(!users.is_empty(), "No users found");

    let theme = ColorfulTheme::default();
    let mut selection = Select::with_theme(&theme);
    for user in &users {
        selection.item(user.name().to_string_lossy());
    }

    let user = selection.default(0).with_prompt("Select user:").interact()?;
    let user = users[user].clone();

    loop {
        let password = Password::with_theme(&theme)
            .with_prompt("Enter password:")
            .interact()
            .map(Zeroizing::new)?;

        if user.check_password(&password)? {
            break Ok(user);
        } else {
            println!("Invalid password!");
        }
    }
}

pub fn select_desktop(user: &User, config: &Config) -> Result<Desktop> {
    let desktops = Desktop::all();
    anyhow::ensure!(!desktops.is_empty(), "No desktops found");

    let last_desktop = user.get_last_desktop(&desktops);
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

struct Env {
    runtime_dir: String,
    variables: HashMap<&'static str, OsString>,
}

impl Env {
    fn define(user: &User, desktop: &Desktop) -> Result<Self> {
        let runtime_dir = format!("/run/user/{}", user.uid());

        let mut env: HashMap<&'static str, OsString> = HashMap::new();

        env.insert("HOME", user.home_dir().into());
        env.insert("PWD", user.home_dir().into());
        env.insert("USER", user.name().into());
        env.insert("LOGNAME", user.name().into());
        env.insert("XDG_CONFIG_HOME", user.home_dir().join(".config").into());
        env.insert("XDG_RUNTIME_DIR", runtime_dir.clone().into());
        env.insert("XDG_SEAT", "seat0".into());
        env.insert("XDG_SESSION_CLASS", "user".into());
        env.insert("SHELL", user.shell().into());
        env.insert("LANG", "en_US.UTF-8".into());
        env.insert("PATH", std::env::var("PATH")?.into());

        env.insert("DESKTOP_SESSION", desktop.name.clone().into());
        env.insert("XDG_SESSION_DESKTOP", desktop.name.clone().into());

        if !Path::new(&runtime_dir).exists() {
            std::fs::create_dir_all(&runtime_dir)?;
            user.set_file_owner(&runtime_dir)?;
        }

        std::env::set_current_dir(user.home_dir())?;

        Ok(Self {
            runtime_dir,
            variables: env,
        })
    }
}

fn update_last_session(user: &User, last_desktop: Option<&Desktop>, desktop: &Desktop) {
    let last_session_changed = match last_desktop {
        Some(last_desktop) => last_desktop.exec != desktop.exec,
        None => true,
    };

    if last_session_changed {
        if let Err(e) = user.set_last_session(desktop) {
            eprintln!("Failed to set last session: {}", e);
        }
    }
}

fn xorg(user: User, desktop: Desktop, mut env: Env, config: Config) -> Result<()> {
    let Some(free_display) = XDisplay::find_free_xdisplay() else {
        anyhow::bail!("There is no free xdisplay");
    };

    let xauthority = Path::new(&env.runtime_dir).join(".Xauthority");
    let display = format!(":{free_display}");

    env.variables.insert("XDG_SESSION_TYPE", "x11".into());
    env.variables.insert("XAUTHORITY", xauthority.as_os_str().to_owned());
    env.variables.insert("DISPLAY", display.clone().into());

    std::env::set_var("XAUTHORITY", &xauthority);
    std::env::set_var("DISPLAY", &display);

    std::fs::write(&xauthority, "")?;
    user.set_file_owner(&xauthority)?;

    // Generate mcookie
    let output = exec_cmd("/usr/bin/mcookie", &user, &env).output()?;
    let mcookie = String::from_utf8(output.stdout)?;

    // Generate xauth
    let _output = exec_cmd("/usr/bin/xauth", &user, &env)
        .arg("add")
        .arg(&display)
        .arg(".")
        .arg(mcookie.trim())
        .output()?;

    // Start XOrg
    let xorg = Command::new("/usr/bin/Xorg")
        .arg(format!("vt{}", config.tty))
        .arg(&display)
        .envs(std::env::vars())
        .spawn()?;
    println!("Started Xorg");

    let start_xinit = || -> Result<(XDisplay, Child)> {
        let display = XDisplay::open(&display)?;

        // Start xinit
        let xinit = prepare_gui_command(&user, &desktop, &env, &config)?
            .current_dir(user.home_dir())
            .spawn()?;
        println!("Started XInit");

        Ok((display, xinit))
    };
    let (_xdisplay, xinit) = match start_xinit() {
        Ok(r) => r,
        Err(e) => {
            kill_process(xorg.id());
            wait_process(xorg);
            return Err(e);
        }
    };

    let mut signals = Signals::new([SIGHUP, SIGINT, SIGQUIT, SIGTERM]).unwrap();
    let signals_handle = signals.handle();
    std::thread::spawn({
        let xinit_id = xinit.id();
        move || {
            for _ in signals.forever() {
                kill_process(xinit_id);
            }
        }
    });

    loop {
        let r = unsafe { libc::getpgid(xinit.id() as i32) };
        if r < 0 {
            break;
        } else {
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    wait_process(xinit);
    println!("XInit finished");

    signals_handle.close();

    kill_process(xorg.id());
    wait_process(xorg);
    println!("XOrg finished");

    // Remove auth
    std::fs::remove_file(&xauthority)?;

    Ok(())
}

fn prepare_gui_command(user: &User, desktop: &Desktop, env: &Env, config: &Config) -> Result<Command> {
    let exec = desktop.exec.split(' ').map(OsString::from).collect::<Vec<_>>();

    let (bin, args): (OsString, Vec<OsString>) = if config.launch_type == LaunchType::XInitRc
        && !exec.contains(&OsString::from(".xinitrc"))
        && user.home_dir().join(".xinitrc").exists()
    {
        let bin = "/bin/bash".into();
        let mut args = vec!["--login".into(), user.home_dir().join(".xinitrc").into_os_string()];
        args.extend(exec.into_iter());
        (bin, args)
    } else if config.launch_type == LaunchType::DBus {
        ("dbus-launch".into(), exec)
    } else {
        let Some((bin, args)) = exec.split_first() else {
            anyhow::bail!("Empty exec");
        };
        (bin.to_owned(), args.to_vec())
    };

    let mut command = exec_cmd(bin, user, env);
    command.args(args);
    Ok(command)
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

fn kill_process(pid: u32) {
    unsafe { libc::kill(pid as i32, libc::SIGINT) };
}

fn wait_process(mut child: Child) {
    if let Err(e) = child.wait() {
        eprintln!("Failed to wait child process: {}", e);
    }
}
