use crate::{shared_mem::SharedRegion, tick_loop::TickLoop};
use jni::objects::{JByteBuffer, JClass, JString};
use jni::sys::{jboolean, jlong, jstring};
use jni::JNIEnv;
use std::sync::Mutex;

static RUNTIME: Mutex<Option<TickLoop>> = Mutex::new(None);

#[no_mangle]
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
        let state_size_usize = state_size as usize;
        if state_size_usize < graphite_api::protocol::OFFSET_COMMAND_QUEUE + 32 {
            return Err("state buffer too small".to_string());
        }
        let state = unsafe {
            SharedRegion::try_from_raw(state_ptr as *mut u8, state_size_usize)
                .map_err(|err| err.to_string())?
        };
        let event_ring =
            unsafe {
                SharedRegion::try_from_raw(event_ring_ptr as *mut u8, event_ring_size as usize)
                    .map_err(|err| err.to_string())?
            };
        
        // Do NOT initialize CommandQueue here. Java-side in SharedMemory.initializeCommandQueue()
        // already initializes head, tail, and capacity. Re-initializing from Rust would cause:
        // 1. Loss of commands if called during operation
        // 2. Data race and UB due to concurrent writes to shared memory without proper synchronization
        // Instead, just verify the queue is accessible (use from_raw_ptr, not from_raw).
        let cmd_queue_ptr = state.command_queue_ptr_mut();
        let _ = unsafe { graphite_api::commands::CommandQueue::from_raw_ptr(cmd_queue_ptr) };
        
        let runtime = TickLoop::new(state, event_ring, std::path::PathBuf::from(mods_dir));
        let mut guard = RUNTIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if guard.is_some() {
            return Err("graphiteInit already called (or not cleaned up after shutdown)".to_string());
        }
        runtime.start();
        *guard = Some(runtime);

        Ok::<(), String>(())
    });

    match result {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            let _ = env.throw_new("java/lang/RuntimeException", err);
        }
        Err(_) => {
            let _ = env.throw_new("java/lang/RuntimeException", "Graphite panic in graphiteInit");
        }
    }
}

#[no_mangle]
pub unsafe extern "system" fn Java_dev_graphite_host_NativeBridge_graphiteTick(
    _env: JNIEnv,
    _class: JClass,
    tick_id: jlong,
) {
    if let Some(runtime) = RUNTIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_ref() {
        runtime.signal_tick(tick_id as u64);
    }
}

#[no_mangle]
pub unsafe extern "system" fn Java_dev_graphite_host_NativeBridge_graphiteShutdown(
    _env: JNIEnv,
    _class: JClass,
) {
    let runtime = RUNTIME
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take();
    if let Some(runtime) = runtime {
        runtime.shutdown();
    }
}

#[no_mangle]
pub unsafe extern "system" fn Java_dev_graphite_host_NativeBridge_graphiteDebugInfo(
    env: JNIEnv,
    _class: JClass,
) -> jstring {
    let info = format!(
        "Graphite v{} | Rust {} | platform: {} | runtime: {}",
        env!("CARGO_PKG_VERSION"),
        rustc_version_runtime::version(),
        std::env::consts::OS,
        RUNTIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_ref().map(|runtime| runtime.stats()).unwrap_or_else(|| "not initialized".into())
    );

    env.new_string(info)
        .expect("failed to create Java string")
        .into_raw()
}

#[no_mangle]
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
        .lock().unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
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

#[no_mangle]
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
