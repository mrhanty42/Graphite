mod bridge;
mod mod_loader;
mod shared_mem;
mod tick_loop;

pub use bridge::*;

#[ctor::ctor]
fn init_logger() {
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"),
    )
    .is_test(false)
    .try_init();
    log::info!("[Graphite] Native library initialized");
}
