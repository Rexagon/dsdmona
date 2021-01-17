mod desktop;
mod user;

use desktop::*;

pub struct LastSession {
    pub uid: i32,
    pub exec: String,
    pub env: Environment,
}

pub fn select_desktop() -> Desktop {
    let mut users = unsafe { user::all_users() };
    for user in users {
        println!(
            "User: {}, {}, {}, {}",
            user.uid(),
            user.name().to_string_lossy(),
            user.home_dir().to_string_lossy(),
            user.shell().to_string_lossy()
        );
    }

    // TODO
    Desktop {
        name: "".to_string(),
        exec: "".to_string(),
        env: Environment::Xorg,
        is_user: false,
        path: "".to_string(),
    }
}
