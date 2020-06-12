#![deny(unused_mut)]
#![doc(html_favicon_url = "https://wasmer.io/static/icons/favicon.ico")]
#![doc(html_logo_url = "https://avatars3.githubusercontent.com/u/44205449?s=200&v=4")]

//! Wasmer's WASI implementation
//!
//! Use `generate_import_object` to create an [`ImportObject`].  This [`ImportObject`]
//! can be combined with a module to create an `Instance` which can execute WASI
//! Wasm functions.
//!
//! See `state` for the experimental WASI FS API.  Also see the
//! [WASI plugin example](https://github.com/wasmerio/wasmer/blob/master/examples/plugin.rs)
//! for an example of how to extend WASI using the WASI FS API.

#[macro_use]
mod macros;
mod ptr;
mod state;
mod syscalls;
mod utils;

use crate::syscalls::*;

pub use crate::state::{Fd, WasiFile, WasiFs, WasiFsError, WasiState, ALL_RIGHTS, VIRTUAL_ROOT_FD};
pub use crate::syscalls::types;
pub use crate::utils::{get_wasi_version, is_wasi_module, WasiVersion};

use thiserror::Error;
use wasmer::{imports, Function, ImportObject, Memory, Module, Store};

use std::sync::{Arc, Mutex, MutexGuard};

/// This is returned in `RuntimeError`.
/// Use `downcast` or `downcast_ref` to retrieve the `ExitCode`.
#[derive(Error, Debug)]
pub enum WasiError {
    #[error("WASI exited with code: {0}")]
    Exit(syscalls::types::__wasi_exitcode_t),
    #[error("The WASI version could not be determined")]
    UnknownWasiVersion,
}

/// The environment provided to the WASI imports.
/// It
#[derive(Clone)]
pub struct WasiEnv<'a> {
    state: Arc<Mutex<WasiState>>,
    memory: Option<&'a Memory>,
}

impl<'a> WasiEnv<'a> {
    pub fn new(state: WasiState) -> Self {
        Self {
            state: Arc::new(Mutex::new(state)),
            memory: None,
        }
    }

    pub fn import_object(&mut self, module: &Module) -> Result<ImportObject, WasiError> {
        let wasi_version = get_wasi_version(module, false).ok_or(WasiError::UnknownWasiVersion)?;
        Ok(generate_import_object_from_env(
            module.store(),
            self.clone(),
            wasi_version,
        ))
    }

    /// Set the memory
    pub fn set_memory(&mut self, memory: &'a Memory) {
        self.memory = Some(memory);
    }

    /// Get the WASI state
    pub fn state(&self) -> MutexGuard<WasiState> {
        self.state.lock().unwrap()
    }

    /// Get the WASI state (mutable)
    pub fn state_mut(&mut self) -> MutexGuard<WasiState> {
        self.state.lock().unwrap()
    }

    /// Get a reference to the memory
    pub fn memory(&self) -> &Memory {
        self.memory.as_ref().expect("The expected Memory is not attached to the `WasiEnv`. Did you forgot to call wasi_env.set_memory(...)?")
    }

    pub(crate) fn get_memory_and_wasi_state(
        &mut self,
        _mem_index: u32,
    ) -> (&Memory, MutexGuard<WasiState>) {
        let memory = self.memory.as_ref().unwrap();
        let state = self.state.lock().unwrap();
        (memory, state)
    }
}

/// Create an [`ImportObject`] with an existing [`WasiState`]. [`WasiState`]
/// can be constructed from a [`WasiStateBuilder`](state::WasiStateBuilder).
pub fn generate_import_object_from_env(
    store: &Store,
    wasi_env: WasiEnv,
    version: WasiVersion,
) -> ImportObject {
    match version {
        WasiVersion::Snapshot0 => generate_import_object_snapshot0(store, wasi_env),
        WasiVersion::Snapshot1 | WasiVersion::Latest => {
            generate_import_object_snapshot1(store, wasi_env)
        }
    }
}

/// Combines a state generating function with the import list for legacy WASI
fn generate_import_object_snapshot0(store: &Store, env: WasiEnv) -> ImportObject {
    imports! {
        "wasi_unstable" => {
            "args_get" => Function::new_env(store, env.clone(), args_get),
            "args_sizes_get" => Function::new_env(store, env.clone(), args_sizes_get),
            "clock_res_get" => Function::new_env(store, env.clone(), clock_res_get),
            "clock_time_get" => Function::new_env(store, env.clone(), clock_time_get),
            "environ_get" => Function::new_env(store, env.clone(), environ_get),
            "environ_sizes_get" => Function::new_env(store, env.clone(), environ_sizes_get),
            "fd_advise" => Function::new_env(store, env.clone(), fd_advise),
            "fd_allocate" => Function::new_env(store, env.clone(), fd_allocate),
            "fd_close" => Function::new_env(store, env.clone(), fd_close),
            "fd_datasync" => Function::new_env(store, env.clone(), fd_datasync),
            "fd_fdstat_get" => Function::new_env(store, env.clone(), fd_fdstat_get),
            "fd_fdstat_set_flags" => Function::new_env(store, env.clone(), fd_fdstat_set_flags),
            "fd_fdstat_set_rights" => Function::new_env(store, env.clone(), fd_fdstat_set_rights),
            "fd_filestat_get" => Function::new_env(store, env.clone(), legacy::snapshot0::fd_filestat_get),
            "fd_filestat_set_size" => Function::new_env(store, env.clone(), fd_filestat_set_size),
            "fd_filestat_set_times" => Function::new_env(store, env.clone(), fd_filestat_set_times),
            "fd_pread" => Function::new_env(store, env.clone(), fd_pread),
            "fd_prestat_get" => Function::new_env(store, env.clone(), fd_prestat_get),
            "fd_prestat_dir_name" => Function::new_env(store, env.clone(), fd_prestat_dir_name),
            "fd_pwrite" => Function::new_env(store, env.clone(), fd_pwrite),
            "fd_read" => Function::new_env(store, env.clone(), fd_read),
            "fd_readdir" => Function::new_env(store, env.clone(), fd_readdir),
            "fd_renumber" => Function::new_env(store, env.clone(), fd_renumber),
            "fd_seek" => Function::new_env(store, env.clone(), legacy::snapshot0::fd_seek),
            "fd_sync" => Function::new_env(store, env.clone(), fd_sync),
            "fd_tell" => Function::new_env(store, env.clone(), fd_tell),
            "fd_write" => Function::new_env(store, env.clone(), fd_write),
            "path_create_directory" => Function::new_env(store, env.clone(), path_create_directory),
            "path_filestat_get" => Function::new_env(store, env.clone(), legacy::snapshot0::path_filestat_get),
            "path_filestat_set_times" => Function::new_env(store, env.clone(), path_filestat_set_times),
            "path_link" => Function::new_env(store, env.clone(), path_link),
            "path_open" => Function::new_env(store, env.clone(), path_open),
            "path_readlink" => Function::new_env(store, env.clone(), path_readlink),
            "path_remove_directory" => Function::new_env(store, env.clone(), path_remove_directory),
            "path_rename" => Function::new_env(store, env.clone(), path_rename),
            "path_symlink" => Function::new_env(store, env.clone(), path_symlink),
            "path_unlink_file" => Function::new_env(store, env.clone(), path_unlink_file),
            "poll_oneoff" => Function::new_env(store, env.clone(), legacy::snapshot0::poll_oneoff),
            "proc_exit" => Function::new_env(store, env.clone(), proc_exit),
            "proc_raise" => Function::new_env(store, env.clone(), proc_raise),
            "random_get" => Function::new_env(store, env.clone(), random_get),
            "sched_yield" => Function::new_env(store, env.clone(), sched_yield),
            "sock_recv" => Function::new_env(store, env.clone(), sock_recv),
            "sock_send" => Function::new_env(store, env.clone(), sock_send),
            "sock_shutdown" => Function::new_env(store, env.clone(), sock_shutdown),
        },
    }
}

/// Combines a state generating function with the import list for snapshot 1
fn generate_import_object_snapshot1(store: &Store, env: WasiEnv) -> ImportObject {
    imports! {
        "wasi_snapshot_preview1" => {
            "args_get" => Function::new_env(store, env.clone(), args_get),
            "args_sizes_get" => Function::new_env(store, env.clone(), args_sizes_get),
            "clock_res_get" => Function::new_env(store, env.clone(), clock_res_get),
            "clock_time_get" => Function::new_env(store, env.clone(), clock_time_get),
            "environ_get" => Function::new_env(store, env.clone(), environ_get),
            "environ_sizes_get" => Function::new_env(store, env.clone(), environ_sizes_get),
            "fd_advise" => Function::new_env(store, env.clone(), fd_advise),
            "fd_allocate" => Function::new_env(store, env.clone(), fd_allocate),
            "fd_close" => Function::new_env(store, env.clone(), fd_close),
            "fd_datasync" => Function::new_env(store, env.clone(), fd_datasync),
            "fd_fdstat_get" => Function::new_env(store, env.clone(), fd_fdstat_get),
            "fd_fdstat_set_flags" => Function::new_env(store, env.clone(), fd_fdstat_set_flags),
            "fd_fdstat_set_rights" => Function::new_env(store, env.clone(), fd_fdstat_set_rights),
            "fd_filestat_get" => Function::new_env(store, env.clone(), fd_filestat_get),
            "fd_filestat_set_size" => Function::new_env(store, env.clone(), fd_filestat_set_size),
            "fd_filestat_set_times" => Function::new_env(store, env.clone(), fd_filestat_set_times),
            "fd_pread" => Function::new_env(store, env.clone(), fd_pread),
            "fd_prestat_get" => Function::new_env(store, env.clone(), fd_prestat_get),
            "fd_prestat_dir_name" => Function::new_env(store, env.clone(), fd_prestat_dir_name),
            "fd_pwrite" => Function::new_env(store, env.clone(), fd_pwrite),
            "fd_read" => Function::new_env(store, env.clone(), fd_read),
            "fd_readdir" => Function::new_env(store, env.clone(), fd_readdir),
            "fd_renumber" => Function::new_env(store, env.clone(), fd_renumber),
            "fd_seek" => Function::new_env(store, env.clone(), fd_seek),
            "fd_sync" => Function::new_env(store, env.clone(), fd_sync),
            "fd_tell" => Function::new_env(store, env.clone(), fd_tell),
            "fd_write" => Function::new_env(store, env.clone(), fd_write),
            "path_create_directory" => Function::new_env(store, env.clone(), path_create_directory),
            "path_filestat_get" => Function::new_env(store, env.clone(), path_filestat_get),
            "path_filestat_set_times" => Function::new_env(store, env.clone(), path_filestat_set_times),
            "path_link" => Function::new_env(store, env.clone(), path_link),
            "path_open" => Function::new_env(store, env.clone(), path_open),
            "path_readlink" => Function::new_env(store, env.clone(), path_readlink),
            "path_remove_directory" => Function::new_env(store, env.clone(), path_remove_directory),
            "path_rename" => Function::new_env(store, env.clone(), path_rename),
            "path_symlink" => Function::new_env(store, env.clone(), path_symlink),
            "path_unlink_file" => Function::new_env(store, env.clone(), path_unlink_file),
            "poll_oneoff" => Function::new_env(store, env.clone(), poll_oneoff),
            "proc_exit" => Function::new_env(store, env.clone(), proc_exit),
            "proc_raise" => Function::new_env(store, env.clone(), proc_raise),
            "random_get" => Function::new_env(store, env.clone(), random_get),
            "sched_yield" => Function::new_env(store, env.clone(), sched_yield),
            "sock_recv" => Function::new_env(store, env.clone(), sock_recv),
            "sock_send" => Function::new_env(store, env.clone(), sock_send),
            "sock_shutdown" => Function::new_env(store, env.clone(), sock_shutdown),
        }
    }
}
