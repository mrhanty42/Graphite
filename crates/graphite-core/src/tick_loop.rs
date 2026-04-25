use crate::{mod_loader::ModLoader, shared_mem::SharedRegion};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{Receiver, SyncSender},
        Arc, Condvar, Mutex,
    },
    thread::JoinHandle,
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
    tick_thread: Mutex<Option<JoinHandle<()>>>,
    watcher_thread: Mutex<Option<JoinHandle<()>>>,
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
            tick_thread: Mutex::new(None),
            watcher_thread: Mutex::new(None),
        }
    }

    pub fn start(&self) {
        self.running.store(true, Ordering::Release);

        // Create mods directory synchronously BEFORE starting the file watcher.
        // The tick thread will also try to create it, but having it exist beforehand ensures
        // the watcher.watch() call succeeds on first file watch attempt.
        // Without this, the watcher fails silently if the directory doesn't exist yet,
        // permanently disabling hot reload functionality.
        if let Err(err) = std::fs::create_dir_all(&self.mods_dir) {
            log::warn!("[Graphite] failed to create mods directory: {err}");
        }

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

        let tick_handle = std::thread::Builder::new()
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
                            let mut guard = lock.lock().expect("tick mutex poisoned");
                            while *guard == last_tick
                                && running.load(Ordering::Acquire)
                                && !state.snapshot_ready()
                            {
                                let (next_guard, _timeout) = cvar
                                    .wait_timeout_while(
                                        guard,
                                        std::time::Duration::from_millis(1),
                                        |tick| {
                                            *tick == last_tick
                                                && running.load(Ordering::Acquire)
                                                && !state.snapshot_ready()
                                        },
                                    )
                                    .expect("tick condvar poisoned");
                                guard = next_guard;
                            }
                            *guard
                        };

                        if !running.load(Ordering::Acquire) {
                            break;
                        }

                        last_tick = current_tick;

                        while let Ok(changed_path) = reload_rx.try_recv() {
                            match loader.reload(&changed_path) {
                                Ok(()) => log::info!("[Graphite] hot reload: {}", changed_path.display()),
                                Err(err) => log::error!("[Graphite] hot reload failed: {err}"),
                            }
                            mod_count.store(loader.mod_count() as u64, Ordering::Relaxed);
                        }

                        let tail_before = state.command_queue_tail();

                        let tick_result =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
                                loader.tick_all(world_ptr, cmd_ptr, current_tick);
                            }));
                        if tick_result.is_err() {
                            log::error!(
                                "[Graphite] mod dispatch panicked at tick {}",
                                current_tick
                            );
                        }

                        let tail_after = state.command_queue_tail();
                        let new_bytes = tail_after.wrapping_sub(tail_before);
                        state.set_command_count(new_bytes);
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

        *self
            .tick_thread
            .lock()
            .expect("tick_thread mutex poisoned") = Some(tick_handle);

        self.start_file_watcher();
    }

    fn start_file_watcher(&self) {
        use notify::{event::ModifyKind, EventKind, RecursiveMode, Watcher};

        let mods_dir = self.mods_dir.clone();
        let reload_tx = self.reload_tx.clone();
        let running = Arc::clone(&self.running);

        let watcher_handle = std::thread::Builder::new()
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
                                EventKind::Modify(ModifyKind::Data(_))
                                    | EventKind::Modify(ModifyKind::Name(_))
                                    | EventKind::Create(_)
                            );
                            if !is_write {
                                continue;
                            }

                            for path in event.paths {
                                if crate::utils::has_dynlib_extension(&path) {
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

        *self
            .watcher_thread
            .lock()
            .expect("watcher_thread mutex poisoned") = Some(watcher_handle);
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

        // Wait for both threads to finish
        if let Some(handle) = self
            .tick_thread
            .lock()
            .expect("tick_thread mutex poisoned")
            .take()
        {
            let _ = handle.join();
        }

        if let Some(handle) = self
            .watcher_thread
            .lock()
            .expect("watcher_thread mutex poisoned")
            .take()
        {
            let _ = handle.join();
        }
    }

    pub fn stats(&self) -> String {
        format!(
            "ticks={} | mods={}",
            self.tick_count.load(Ordering::Relaxed),
            self.mod_count.load(Ordering::Relaxed)
        )
    }
}
