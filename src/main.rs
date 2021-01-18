use anyhow::Result;

use dsdmona::{Config, LaunchType};

fn main() -> Result<()> {
    dsdmona::login(Config {
        tty: 1,
        launch_type: LaunchType::DBus,
        auto_login_session: None,
    })
}
