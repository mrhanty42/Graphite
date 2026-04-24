use crate::{mod_loader::ModLoader, shared_mem::SharedRegion};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{Receiver, SyncSender},
        Arc, Condvar, Mutex,
    },
};

pub struct TickLoop {
    wakeup: Arc<(Mutex<u64>, Condvar)>,
    running: Arc<AtomicBool>,
    state: Arc<SharedRegion>,
    mods_dir: PathBuf,
    reload_tx: SyncSender<PathBuf>,
    reload_rx: Mutex<Option<Receiver<PathBuf>>>,
    tick_count: Arc<AtomicU64>,
    mod_count: Arc<AtomicU64>,
}

impl TickLoop {
    pub fn new(state: SharedRegion, _event_ring: SharedRegion, mods_dir: PathBuf) -> Self {
        let (reload_tx, reload_rx) = std::sync::mpsc::sync_channel(32);
        Self {
            wakeup: Arc::new((Mutex::new(0), Condvar::new())),
            running: Arc::new(AtomicBool::new(false)),
            state: Arc::new(state),
            mods_dir,
            reload_tx,
            reload_rx: Mutex::new(Some(reload_rx)),
            tick_count: Arc::new(AtomicU64::new(0)),
            mod_count: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn start(&self) {
        self.running.store(true, Ordering::Release);

        let wakeup = Arc::clone(&self.wakeup);
        let running = Arc::clone(&self.running);
        let state = Arc::clone(&self.state);
        let mods_dir = self.mods_dir.clone();
        let reload_rx = self
            .reload_rx
            .lock()
            .expect("reload receiver mutex poisoned")
            .take()
            .expect("tick loop already started");
        let tick_count = Arc::clone(&self.tick_count);
        let mod_count = Arc::clone(&self.mod_count);

        std::thread::Builder::new()
            .name("graphite-tick".into())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut loader = ModLoader::new(mods_dir.clone());
                    loader.scan_and_load();
                    mod_count.store(loader.mod_count() as u64, Ordering::Relaxed);

                    if loader.mod_count() == 0 {
                        log::warn!("[Graphite] mod directory is empty: {}", mods_dir.display());
                    } else {
                        log::info!("[Graphite] loaded mods: {:?}", loader.mod_names());
                    }

                    let world_ptr = state.world_snapshot_ptr();
                    let cmd_ptr = state.command_queue_ptr_mut();
                    let mut last_tick = 0_u64;

                    while running.load(Ordering::Acquire) {
                        let current_tick = {
                            let (lock, cvar) = &*wakeup;
                            let guard = lock.lock().expect("tick mutex poisoned");
                            let guard = cvar
                                .wait_while(guard, |tick| {
                                    *tick == last_tick && running.load(Ordering::Acquire)
                                })
                                .expect("tick condvar poisoned");
                            *guard
                        };

                        if !running.load(Ordering::Acquire) {
                            break;
                        }
                        last_tick = current_tick;

                        if !state.snapshot_ready() {
                            state.set_command_count(0);
                            continue;
                        }

                        while let Ok(changed_path) = reload_rx.try_recv() {
                            match loader.reload(&changed_path) {
                                Ok(()) => log::info!("[Graphite] hot reload: {}", changed_path.display()),
                                Err(err) => log::error!("[Graphite] hot reload failed: {err}"),
                            }
                            mod_count.store(loader.mod_count() as u64, Ordering::Relaxed);
                        }

                        unsafe {
                            loader.tick_all(world_ptr, cmd_ptr, current_tick);
                        }

                        state.set_command_count(0);
                        state.clear_snapshot_ready();
                        tick_count.fetch_add(1, Ordering::Relaxed);
                    }
                }));

                if result.is_err() {
                    log::error!("[Graphite] tick loop panicked and was aborted");
                }

                log::info!(
                    "[Graphite] tick loop stopped after {} ticks",
                    tick_count.load(Ordering::Relaxed)
                );
            })
            .expect("failed to spawn graphite-tick");

        self.start_file_watcher();
    }

    fn start_file_watcher(&self) {
        use notify::{event::ModifyKind, EventKind, RecursiveMode, Watcher};

        let mods_dir = self.mods_dir.clone();
        let reload_tx = self.reload_tx.clone();
        let running = Arc::clone(&self.running);

        std::thread::Builder::new()
            .name("graphite-watcher".into())
            .spawn(move || {
                let (tx, rx) = std::sync::mpsc::channel();
                let mut watcher = match notify::recommended_watcher(tx) {
                    Ok(watcher) => watcher,
                    Err(err) => {
                        log::warn!("[Graphite] watcher unavailable: {err}");
                        return;
                    }
                };

                if let Err(err) = watcher.watch(&mods_dir, RecursiveMode::NonRecursive) {
                    log::warn!("[Graphite] failed to watch {}: {err}", mods_dir.display());
                    return;
                }

                while running.load(Ordering::Acquire) {
                    match rx.recv_timeout(std::time::Duration::from_secs(1)) {
                        Ok(Ok(event)) => {
                            let is_write = matches!(
                                event.kind,
                                EventKind::Modify(ModifyKind::Data(_)) | EventKind::Create(_)
                            );
                            if !is_write {
                                continue;
                            }

                            for path in event.paths {
                                if has_dynlib_extension(&path) {
                                    let _ = reload_tx.try_send(path);
                                }
                            }
                        }
                        Ok(Err(err)) => log::warn!("[Graphite] watcher event error: {err}"),
                        Err(_) => {}
                    }
                }
            })
            .expect("failed to spawn graphite-watcher");
    }

    #[inline]
    pub fn signal_tick(&self, tick_id: u64) {
        let (lock, cvar) = &*self.wakeup;
        *lock.lock().expect("tick mutex poisoned") = tick_id;
        cvar.notify_one();
    }

    pub fn request_reload(&self, path: PathBuf) -> Result<(), String> {
        self.reload_tx
            .try_send(path)
            .map_err(|err| format!("reload queue error: {err}"))
    }

    pub fn shutdown(&self) {
        self.running.store(false, Ordering::Release);
        let (lock, cvar) = &*self.wakeup;
        drop(lock.lock().expect("tick mutex poisoned"));
        cvar.notify_all();
    }

    pub fn stats(&self) -> String {
        format!(
            "ticks={} | mods={}",
            self.tick_count.load(Ordering::Relaxed),
            self.mod_count.load(Ordering::Relaxed)
        )
    }
}

fn has_dynlib_extension(path: &std::path::Path) -> bool {
    let expected = std::env::consts::DLL_SUFFIX.trim_start_matches('.');
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}
