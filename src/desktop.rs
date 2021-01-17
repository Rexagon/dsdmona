pub enum Environment {
    Xorg,
}

pub struct Desktop {
    pub name: String,
    pub exec: String,
    pub env: Environment,
    pub is_user: bool,
    pub path: String,
}
