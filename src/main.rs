use adamant;

use env_logger::{self, Env};

fn main() {
    let env = Env::default()
        .filter_or("MY_LOG_LEVEL", "trace")
        .write_style_or("MY_LOG_STYLE", "auto");

    env_logger::init_from_env(env);

    let init_flags = adamant::InitFlags::ALLOW_TEARING;
    adamant::init_d3d12(init_flags);
}
