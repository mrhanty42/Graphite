use graphite_api::{
    mod_trait::{FnModName, FnModVersion, FnOnLoad, FnOnTick, FnOnUnload, ModLoadContext},
    protocol::{ENTITY_RECORD_SIZE, PROTOCOL_VERSION},
};
use libloading::{Library, Symbol};
use std::{
    ffi::{CStr, CString},
    path::{Path, PathBuf},
};

pub struct LoadedMod {
    pub name: String,
    pub version: String,
    path: PathBuf,
    library: Library,
    temp_path: Option<PathBuf>,
    on_tick_fn: FnOnTick,
}

impl LoadedMod {
    fn load(path: &Path, mods_dir: &Path, load_gen: u64) -> Result<Self, String> {
        let ext = std::env::consts::DLL_EXTENSION;
        cleanup_stale_temp_copies(path)?;
        let temp_root = std::env::temp_dir().join("graphite_mods");
        std::fs::create_dir_all(&temp_root)
            .map_err(|err| format!("failed to create temp dir: {err}"))?;

        // Use NamedTempFile for automatic cleanup if panic occurs before library loading
        let temp_file = tempfile::Builder::new()
            .prefix(&format!("{}_v{}_", path.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("graphite_mod"), load_gen))
            .suffix(&format!(".{ext}"))
            .tempfile_in(&temp_root)
            .map_err(|err| format!("failed to create temp file: {err}"))?;

        let temp_path = temp_file.path().to_path_buf();

        std::fs::copy(path, &temp_path)
            .map_err(|err| format!("failed to copy {}: {err}", path.display()))?;

        // Prevent NamedTempFile from deleting the file on drop - we need it for libloading.
        // into_temp_path() + immediate drop would delete the file before Library::new can load it.
        // std::mem::forget leaks the File handle inside NamedTempFile but prevents file deletion;
        // cleanup is handled manually in LoadedMod::Drop.
        std::mem::forget(temp_file);

        let temp_path_cleanup = temp_path.clone();
        let result: Result<Self, String> = (|| {
            let (library, loaded_from_temp) = match unsafe { Library::new(&temp_path) } {
                Ok(lib) => (lib, true),
                Err(temp_err) => {
                    log::warn!(
                        "[Graphite/Loader] temp copy load failed ({}): {temp_err}; falling back to original path",
                        temp_path.display()
                    );
                    let lib = unsafe {
                        Library::new(path).map_err(|err| {
                            format!(
                                "failed to load {}: {err} (after temp load failure: {temp_err})",
                                path.display()
                            )
                        })?
                    };
                    (lib, false)
                }
            };

            let on_tick_fn: FnOnTick = unsafe {
                let symbol: Symbol<'_, FnOnTick> = library
                    .get(b"graphite_mod_on_tick\0")
                    .map_err(|_| format!("{} missing graphite_mod_on_tick", path.display()))?;
                *symbol
            };

            let name = unsafe {
                let symbol: Symbol<'_, FnModName> = library
                    .get(b"graphite_mod_name\0")
                    .map_err(|_| format!("{} missing graphite_mod_name", path.display()))?;
                CStr::from_ptr(symbol()).to_string_lossy().into_owned()
            };

            let version = unsafe {
                let symbol: Symbol<'_, FnModVersion> = library
                    .get(b"graphite_mod_version\0")
                    .map_err(|_| format!("{} missing graphite_mod_version", path.display()))?;
                CStr::from_ptr(symbol()).to_string_lossy().into_owned()
            };

            let mods_dir_cstr = CString::new(mods_dir.to_string_lossy().as_bytes())
                .map_err(|err| format!("invalid mods dir string: {err}"))?;
            let ctx = ModLoadContext {
                mods_dir: mods_dir_cstr.as_ptr(),
                protocol_version: PROTOCOL_VERSION,
                entity_record_size: ENTITY_RECORD_SIZE as u32,
                reserved: [0_u64; 4],
            };
            unsafe {
                let symbol: Symbol<'_, FnOnLoad> = library
                    .get(b"graphite_mod_on_load\0")
                    .map_err(|_| format!("{} missing graphite_mod_on_load", path.display()))?;
                let result = std::panic::catch_unwind(|| {
                    symbol(&ctx as *const ModLoadContext);
                });
                if result.is_err() {
                    return Err(format!("{} panicked in on_load", path.display()));
                }
            }

            Ok(Self {
                name,
                version,
                path: path.to_path_buf(),
                temp_path: loaded_from_temp.then_some(temp_path),
                library,
                on_tick_fn,
            })
        })();

        if result.is_err() {
            let _ = std::fs::remove_file(&temp_path_cleanup);
        }

        result
    }

    #[inline(always)]
    pub unsafe fn call_on_tick(&self, world_ptr: *const u8, cmd_ptr: *mut u8, tick: u64) {
        unsafe { (self.on_tick_fn)(world_ptr, cmd_ptr, tick) };
    }

    fn call_on_unload(&self) {
        unsafe {
            if let Ok(symbol) = self.library.get::<FnOnUnload>(b"graphite_mod_on_unload\0") {
                (*symbol)();
            }
        }
    }
}

impl Drop for LoadedMod {
    fn drop(&mut self) {
        self.call_on_unload();
        if let Some(temp_path) = &self.temp_path {
            if let Err(err) = std::fs::remove_file(temp_path) {
                log::warn!(
                    "[Graphite] failed to remove temp file '{}': {}",
                    temp_path.display(),
                    err
                );
            }
        }
        log::info!("[Graphite] unloaded mod '{}'", self.name);
    }
}

pub struct ModLoader {
    mods: Vec<LoadedMod>,
    mods_dir: PathBuf,
    load_gen: u64,
}

impl ModLoader {
    pub fn new(mods_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&mods_dir).expect("failed to create mods directory");
        Self {
            mods: Vec::new(),
            mods_dir,
            load_gen: 0,
        }
    }

    pub fn scan_and_load(&mut self) {
        log::info!("[Graphite/Loader] scanning {}", self.mods_dir.display());

        let entries = match std::fs::read_dir(&self.mods_dir) {
            Ok(entries) => entries,
            Err(err) => {
                log::error!("[Graphite/Loader] failed to read mods directory: {err}");
                return;
            }
        };

        let expected_ext = std::env::consts::DLL_EXTENSION;
        log::info!("[Graphite/Loader] expecting .{expected_ext} files");

        for entry in entries.flatten() {
            let path = entry.path();
            log::info!("[Graphite/Loader] found {}", path.display());
            if !crate::utils::has_dynlib_extension(&path) {
                log::debug!(
                    "[Graphite/Loader] skipping {} due to extension mismatch",
                    path.display()
                );
                continue;
            }

            self.load_gen += 1;
            match LoadedMod::load(&path, &self.mods_dir, self.load_gen) {
                Ok(module) => {
                    log::info!("[Graphite/Loader] loaded {} v{}", module.name, module.version);
                    self.mods.push(module);
                }
                Err(err) => {
                    log::error!("[Graphite/Loader] failed to load {}: {err}", path.display())
                }
            }
        }

        log::info!("[Graphite/Loader] total loaded mods: {}", self.mods.len());
    }

    pub fn reload(&mut self, path: &Path) -> Result<(), String> {
        if let Some(index) = self.mods.iter().position(|module| module.path == path) {
            self.mods.remove(index);
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
        self.load_gen += 1;
        let module = LoadedMod::load(path, &self.mods_dir, self.load_gen)?;
        log::info!("[Graphite] hot reloaded {} v{}", module.name, module.version);
        self.mods.push(module);
        Ok(())
    }

    #[inline]
    pub unsafe fn tick_all(&self, world_ptr: *const u8, cmd_ptr: *mut u8, tick_id: u64) {
        for module in &self.mods {
            unsafe { module.call_on_tick(world_ptr, cmd_ptr, tick_id) };
        }
    }

    pub fn mod_count(&self) -> usize {
        self.mods.len()
    }

    pub fn mod_names(&self) -> Vec<String> {
        self.mods
            .iter()
            .map(|module| format!("{} v{}", module.name, module.version))
            .collect()
    }
}


fn cleanup_stale_temp_copies(path: &Path) -> Result<(), String> {
    let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
        return Ok(());
    };

    let temp_root = std::env::temp_dir().join("graphite_mods");
    if !temp_root.exists() {
        return Ok(());
    }

    let prefix = format!("{stem}_v");
    let expected_ext = std::env::consts::DLL_EXTENSION;
    let entries = std::fs::read_dir(&temp_root)
        .map_err(|err| format!("failed to inspect temp mod dir {}: {err}", temp_root.display()))?;

    for entry in entries.flatten() {
        let temp_path = entry.path();
        let matches_prefix = temp_path
            .file_stem()
            .and_then(|value| value.to_str())
            .map(|value| value.starts_with(&prefix))
            .unwrap_or(false);
        let matches_ext = temp_path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case(expected_ext))
            .unwrap_or(false);

        if matches_prefix && matches_ext {
            let _ = std::fs::remove_file(&temp_path);
        }
    }

    Ok(())
}
