mod desktop;
mod path;
mod user;

use desktop::{Desktop, Environment};

pub struct LastSession {
    pub uid: i32,
    pub exec: String,
    pub env: Environment,
}

pub fn select_desktop() -> Option<Desktop> {
    let mut users = unsafe { user::all_human_users() };
    for user in users {
        println!(
            "User: {}, {}, {}, {}",
            user.uid(),
            user.name().to_string_lossy(),
            user.home_dir().to_string_lossy(),
            user.shell().to_string_lossy()
        );
    }

    let desktops = desktop::all_desktops();
    for desktop in desktops {
        println!("{:?}", desktop);
    }

    None
}
