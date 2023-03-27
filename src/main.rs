use anyhow::Result;
use argh::FromArgs;

use dsdmona::{Config, LaunchType};

fn main() -> Result<()> {
    let app: App = argh::from_env();

    dsdmona::login(Config {
        tty: app.tty,
        launch_type: app.launch_type,
        auto_login_session: None,
    })
}

/// Dead simpla display manager.
#[derive(FromArgs)]
struct App {
    /// tty, where dsdmona will start.
    #[argh(option)]
    tty: u8,
    /// how to start the desktop. xinitrc (default) or dbus.
    #[argh(option, default = "LaunchType::XInitRc")]
    launch_type: LaunchType,
}
