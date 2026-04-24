use crate::{shared_mem::SharedRegion, tick_loop::TickLoop};
use jni::objects::{JByteBuffer, JClass, JString};
use jni::sys::{jboolean, jlong, jstring};
use jni::JNIEnv;
use std::sync::OnceLock;

static RUNTIME: OnceLock<TickLoop> = OnceLock::new();

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_graphite_host_NativeBridge_graphiteInit(
    mut env: JNIEnv,
    _class: JClass,
    state_ptr: jlong,
    state_size: jlong,
    event_ring_ptr: jlong,
    event_ring_size: jlong,
    mods_dir_jstr: JString,
) {
    let mods_dir: String = env
        .get_string(&mods_dir_jstr)
        .expect("failed to read mods_dir")
        .into();

    let result = std::panic::catch_unwind(|| {
        let state = unsafe { SharedRegion::from_raw(state_ptr as *mut u8, state_size as usize) };
        let event_ring =
            unsafe { SharedRegion::from_raw(event_ring_ptr as *mut u8, event_ring_size as usize) };
        let runtime = TickLoop::new(state, event_ring, std::path::PathBuf::from(mods_dir));
        assert!(RUNTIME.set(runtime).is_ok(), "graphiteInit called twice");
        RUNTIME.get().expect("runtime just set").start();
    });

    if result.is_err() {
        let _ = env.throw_new("java/lang/RuntimeException", "Graphite panic in graphiteInit");
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_graphite_host_NativeBridge_graphiteTick(
    _env: JNIEnv,
    _class: JClass,
    tick_id: jlong,
) {
    if let Some(runtime) = RUNTIME.get() {
        runtime.signal_tick(tick_id as u64);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_graphite_host_NativeBridge_graphiteShutdown(
    _env: JNIEnv,
    _class: JClass,
) {
    if let Some(runtime) = RUNTIME.get() {
        runtime.shutdown();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_graphite_host_NativeBridge_graphiteDebugInfo(
    env: JNIEnv,
    _class: JClass,
) -> jstring {
    let info = format!(
        "Graphite v{} | Rust {} | platform: {} | runtime: {}",
        env!("CARGO_PKG_VERSION"),
        rustc_version_runtime::version(),
        std::env::consts::OS,
        RUNTIME.get().map(|runtime| runtime.stats()).unwrap_or_else(|| "not initialized".into())
    );

    env.new_string(info)
        .expect("failed to create Java string")
        .into_raw()
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_graphite_host_NativeBridge_graphiteReloadMod(
    mut env: JNIEnv,
    _class: JClass,
    lib_path: JString,
) -> jboolean {
    let path: String = env
        .get_string(&lib_path)
        .expect("failed to read lib path")
        .into();

    match RUNTIME
        .get()
        .map(|runtime| runtime.request_reload(std::path::PathBuf::from(path)))
    {
        Some(Ok(())) => 1,
        Some(Err(err)) => {
            log::error!("[Graphite] reload failed: {err}");
            0
        }
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_graphite_host_NativeBridge_graphiteGetDirectBufferAddress(
    env: JNIEnv,
    _class: JClass,
    buffer: JByteBuffer,
) -> jlong {
    match env.get_direct_buffer_address(&buffer) {
        Ok(ptr) => ptr as jlong,
        Err(err) => {
            log::error!("[Graphite] failed to get direct buffer address: {err}");
            0
        }
    }
}
