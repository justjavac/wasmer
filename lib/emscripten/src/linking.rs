use wasmer_runtime_core::vm::Ctx;

// TODO: Need to implement.

/// emscripten: dlopen(filename: *const c_char, flag: c_int) -> *mut c_void
pub extern "C" fn _dlopen(filename: u32, flag: u32, _ctx: &mut Ctx) -> i32 {
    debug!("emscripten::_dlopen");
    -1
}

/// emscripten: dlclose(handle: *mut c_void) -> c_int
pub extern "C" fn _dlclose(filename: u32, _ctx: &mut Ctx) -> i32 {
    debug!("emscripten::_dlclose");
    -1
}

/// emscripten: dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void
pub extern "C" fn _dlsym(filepath: u32, symbol: u32, _ctx: &mut Ctx) -> i32 {
    debug!("emscripten::_dlsym");
    -1
}

/// emscripten: dlerror() -> *mut c_char
pub extern "C" fn _dlerror(_ctx: &mut Ctx) -> i32 {
    debug!("emscripten::_dlerror");
    -1
}
