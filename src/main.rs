use anyhow::Result;

use dsdmona::{Config, LaunchType};

fn main() -> Result<()> {
    dsdmona::login(Config {
        tty: 2,
        launch_type: LaunchType::XInitRc,
        auto_login_session: None,
    })
}
