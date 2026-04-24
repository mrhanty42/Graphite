use crate::{commands::CommandQueue, world::WorldView};

pub type FnModName = unsafe extern "C" fn() -> *const std::ffi::c_char;
pub type FnModVersion = unsafe extern "C" fn() -> *const std::ffi::c_char;
pub type FnOnLoad = unsafe extern "C" fn(ctx: *const ModLoadContext);
pub type FnOnTick = unsafe extern "C" fn(world_ptr: *const u8, cmd_ptr: *mut u8, tick_id: u64);
pub type FnOnUnload = unsafe extern "C" fn();

#[repr(C)]
pub struct ModLoadContext {
    pub mods_dir: *const std::ffi::c_char,
    pub protocol_version: u32,
    pub entity_record_size: u32,
    pub reserved: [u64; 4],
}

pub trait GraphiteModImpl: Send + 'static {
    fn new() -> Self;
    fn on_load(&mut self, ctx: *const ModLoadContext);
    fn on_tick(&mut self, world: &WorldView, cmd: &mut CommandQueue, tick: u64);
    fn on_unload(&mut self) {}
}

#[macro_export]
macro_rules! graphite_mod {
    (name: $name:literal, version: $ver:literal, type: $ty:ty $(,)?) => {
        static MOD_INSTANCE: ::std::sync::OnceLock<::std::sync::Mutex<$ty>> =
            ::std::sync::OnceLock::new();

        #[unsafe(no_mangle)]
        pub extern "C" fn graphite_mod_name() -> *const ::std::ffi::c_char {
            concat!($name, "\0").as_ptr() as *const ::std::ffi::c_char
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn graphite_mod_version() -> *const ::std::ffi::c_char {
            concat!($ver, "\0").as_ptr() as *const ::std::ffi::c_char
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn graphite_mod_on_load(
            ctx: *const $crate::mod_trait::ModLoadContext,
        ) {
            MOD_INSTANCE.get_or_init(|| {
                let instance = <$ty as $crate::mod_trait::GraphiteModImpl>::new();
                ::std::sync::Mutex::new(instance)
            });

            if let Some(instance) = MOD_INSTANCE.get() {
                if let Ok(mut guard) = instance.lock() {
                    guard.on_load(ctx);
                }
            }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn graphite_mod_on_tick(
            world_ptr: *const u8,
            cmd_ptr: *mut u8,
            tick_id: u64,
        ) {
            let world = unsafe { $crate::world::WorldView::from_raw(world_ptr) };
            let mut cmd = unsafe { $crate::commands::CommandQueue::from_raw_ptr(cmd_ptr) };

            if let Some(instance) = MOD_INSTANCE.get() {
                if let Ok(mut guard) = instance.try_lock() {
                    guard.on_tick(&world, &mut cmd, tick_id);
                }
            }
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn graphite_mod_on_unload() {
            if let Some(instance) = MOD_INSTANCE.get() {
                if let Ok(mut guard) = instance.lock() {
                    guard.on_unload();
                }
            }
            ::log::info!("[{}] unloaded", $name);
        }
    };
}
