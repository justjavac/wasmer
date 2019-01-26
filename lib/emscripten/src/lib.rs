#[macro_use]
extern crate wasmer_runtime_core;

use byteorder::{ByteOrder, LittleEndian};
use libc::c_int;
use std::cell::UnsafeCell;
use std::{ffi::c_void, mem, ptr};
use std::{fmt, mem::size_of, slice};
use wasmer_runtime_core::{
    error::{CallResult, ResolveError},
    export::{Context, Export, FuncPointer, GlobalPointer, MemoryPointer, TablePointer},
    import::{ImportObject, Namespace},
    instance::Instance,
    memory::LinearMemory,
    module::Module,
    structures::TypedIndex,
    table::TableBacking,
    types::{
        ElementType, FuncSig, GlobalDesc, LocalMemoryIndex, Memory, Table,
        Type::{self, *},
        Value,
    },
    vm::Ctx,
    vm::LocalGlobal,
    vm::LocalMemory,
    vm::LocalTable,
};

#[macro_use]
mod macros;
//#[cfg(test)]
mod file_descriptor;
pub mod stdio;

// EMSCRIPTEN APIS
mod env;
mod errno;
mod exception;
mod io;
mod jmp;
mod linking;
mod lock;
mod math;
mod memory;
mod nullfunc;
mod process;
mod signal;
mod storage;
mod syscalls;
mod time;
mod utils;
mod varargs;

pub use self::storage::align_memory;
pub use self::utils::{
    allocate_cstr_on_stack, allocate_on_stack, get_emscripten_memory_size,
    get_emscripten_table_size, is_emscripten_module,
};

// TODO: Magic number - how is this calculated?
const TOTAL_STACK: u32 = 5_242_880;
// TODO: Magic number - how is this calculated?
const DYNAMICTOP_PTR_DIFF: u32 = 1088;
// TODO: make this variable
const STATIC_BUMP: u32 = 215_536;

// The address globals begin at. Very low in memory, for code size and optimization opportunities.
// Above 0 is static memory, starting with globals.
// Then the stack.
// Then 'dynamic' memory for sbrk.
const GLOBAL_BASE: u32 = 1024;
const STATIC_BASE: u32 = GLOBAL_BASE;

fn stacktop(static_bump: u32) -> u32 {
    align_memory(dynamictop_ptr(static_bump) + 4)
}

fn stack_max(static_bump: u32) -> u32 {
    stacktop(static_bump) + TOTAL_STACK
}

fn dynamic_base(static_bump: u32) -> u32 {
    align_memory(stack_max(static_bump))
}

fn dynamictop_ptr(static_bump: u32) -> u32 {
    static_bump + DYNAMICTOP_PTR_DIFF
}

pub struct EmscriptenData {
    pub malloc: extern "C" fn(i32, &mut Ctx) -> u32,
    pub free: extern "C" fn(i32, &mut Ctx),
    pub memalign: extern "C" fn(u32, u32, &mut Ctx) -> u32,
    pub memset: extern "C" fn(u32, i32, u32, &mut Ctx) -> u32,
    pub stack_alloc: extern "C" fn(u32, &mut Ctx) -> u32,
    pub jumps: Vec<UnsafeCell<[c_int; 27]>>,
}

impl EmscriptenData {
    pub fn new(instance: &mut Instance) -> Self {
        unsafe {
            let malloc_func = instance.func("_malloc");
            let malloc_addr = if let Ok(malloc_func) = malloc_func {
                malloc_func.raw() as *const u8
            } else {
                0 as *const u8
            };
            let free_func = instance.func("_free");
            let free_addr = if let Ok(free_func) = free_func {
                free_func.raw() as *const u8
            } else {
                0 as *const u8
            };
            let memalign_func = instance.func("_memalign");
            let memalign_addr = if let Ok(memalign_func) = memalign_func {
                memalign_func.raw() as *const u8
            } else {
                0 as *const u8
            };
            let memset_func = instance.func("_memset");
            let memset_addr = if let Ok(memset_func) = memset_func {
                memset_func.raw() as *const u8
            } else {
                0 as *const u8
            };
            let stack_alloc_func = instance.func("stackAlloc");
            let stack_alloc_addr = if let Ok(stack_alloc_func) = stack_alloc_func {
                stack_alloc_func.raw() as *const u8
            } else {
                0 as *const u8
            };

            EmscriptenData {
                malloc: mem::transmute(malloc_addr),
                free: mem::transmute(free_addr),
                memalign: mem::transmute(memalign_addr),
                memset: mem::transmute(memset_addr),
                stack_alloc: mem::transmute(stack_alloc_addr),
                jumps: Vec::new(),
            }
        }
    }
}

impl fmt::Debug for EmscriptenData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("EmscriptenData")
            .field("malloc", &(self.malloc as usize))
            .field("free", &(self.free as usize))
            .finish()
    }
}

pub fn run_emscripten_instance(
    _module: &Module,
    instance: &mut Instance,
    path: &str,
    args: Vec<&str>,
) -> CallResult<()> {
    let mut data = EmscriptenData::new(instance);
    let data_ptr = &mut data as *mut _ as *mut c_void;
    instance.ctx().data = data_ptr;

    let main_func = instance.func("_main")?;
    let num_params = main_func.signature().params.len();
    let result = match num_params {
        2 => {
            let (argc, argv) = (5, 217920); // TODO Fix
                                            // TODO store_module_arguments, cannot borrow `*ctx` as mutable more than once at a time
                                            // store_module_arguments(path, args, instance.ctx());
            instance.call("_main", &[Value::I32(argc as i32), Value::I32(argv as i32)])?;
        }
        0 => {
            instance.call("_main", &[])?;
        }
        _ => panic!(
            "The emscripten main function has received an incorrect number of params {}",
            num_params
        ),
    };

    // TODO atinit and atexit for emscripten
    println!("{:?}", data);
    Ok(())
}

//fn store_module_arguments(path: &str, args: Vec<&str>, ctx: &mut Ctx) -> (u32, u32) {
//    let argc = args.len() + 1;
//
//    let (argv_offset, argv_slice): (_, &mut [u32]) =
//        unsafe { allocate_on_stack(((argc + 1) * 4) as u32, ctx) };
//    assert!(!argv_slice.is_empty());
//
//    argv_slice[0] = unsafe { allocate_cstr_on_stack(path, ctx).0 };
//
//    for (slot, arg) in argv_slice[1..argc].iter_mut().zip(args.iter()) {
//        *slot = unsafe { allocate_cstr_on_stack(&arg, ctx).0 };
//    }
//
//    argv_slice[argc] = 0;
//
//    (argc as u32, argv_offset)
//}

/// Passes arguments from the host to the WebAssembly instance.
fn get_main_args(
    main_name: &str,
    args: Vec<&str>,
    instance: &mut Instance,
) -> CallResult<Vec<Value>> {
    // Getting main function signature.

    let func = instance.func(main_name)?;
    let func_sig = func.signature();
    let params = &func_sig.params;

    // Check for a () or (i32, i32) sig.
    match params.as_slice() {
        &[Type::I32, Type::I32] => {
            // Copy strings into wasm memory and get addresses to them.
            let string_addresses = args
                .iter()
                .map(|string| copy_string_into_wasm(instance, (*string).to_string()).unwrap())
                .collect();

            // Create a wasm array to the strings.
            let array = create_wasm_array(instance, string_addresses).unwrap();

            Ok(vec![
                Value::I32(array as i32),
                Value::I32(args.len() as i32),
            ])
        }
        &[] => Ok(vec![]),
        _ => Err(ResolveError::Signature {
            expected: FuncSig {
                params: vec![Type::I32, Type::I32],
                returns: vec![],
            },
            found: params.to_vec(),
        }
        .into()),
    }
}

/// Copy rust string to wasm instance.
fn copy_string_into_wasm(instance: &mut Instance, string: String) -> CallResult<u32> {
    let string_len = string.len();

    let space_offset = instance
        .call("_malloc", &[Value::I32((string_len as i32) + 1)])
        .unwrap();

    let space_offset = match space_offset.as_slice() {
        &[Value::I32(res)] => Some(res as u32),
        _ => None,
    }
    .unwrap();

    let raw_memory = instance.ctx().memory(0)[space_offset as usize] as *mut u8;

    let slice = unsafe { slice::from_raw_parts_mut(raw_memory, string_len) };

    for (byte, loc) in string.bytes().zip(slice.iter_mut()) {
        *loc = byte;
    }

    unsafe { *raw_memory.add(string_len) = 0 };

    Ok(space_offset)
}

/// Create a pointer to an array of items in a wasm memory
fn create_wasm_array(instance: &mut Instance, values: Vec<u32>) -> CallResult<u32> {
    let values_len = values.len();

    // Space to store pointers to values
    let values_offset = instance
        .call(
            "_malloc",
            &[Value::I32((size_of::<u32>() * values.len()) as i32)],
        )
        .unwrap();

    let values_offset = match values_offset.as_slice() {
        &[Value::I32(res)] => Some(res as u32),
        _ => None,
    }
    .unwrap();

    let raw_memory = instance.ctx().memory(0)[values_offset as usize] as *mut u32;

    let slice = unsafe { slice::from_raw_parts_mut(raw_memory, values_len) };

    for (value, loc) in values.iter().zip(slice.iter_mut()) {
        *loc = value.clone();
    }

    // Space to store pointer to array
    let array_offset = instance
        .call("_malloc", &[Value::I32(size_of::<u32>() as i32)])
        .unwrap();

    let array_offset = match array_offset.as_slice() {
        &[Value::I32(res)] => Some(res as u32),
        _ => None,
    }
    .unwrap();

    let raw_memory = instance.ctx().memory(0)[values_offset as usize] as *mut u32;

    unsafe { *raw_memory = values_offset };

    Ok(array_offset)
}

pub fn emscripten_set_up_memory(memory: &mut LinearMemory) {
    let dynamictop_ptr = dynamictop_ptr(STATIC_BUMP) as usize;
    let dynamictop_ptr_offset = dynamictop_ptr + mem::size_of::<u32>();

    // println!("value = {:?}");

    // We avoid failures of setting the u32 in our memory if it's out of bounds
    if dynamictop_ptr_offset > memory.len() {
        return; // TODO: We should panic instead?
    }

    // debug!("###### dynamic_base = {:?}", dynamic_base(STATIC_BUMP));
    // debug!("###### dynamictop_ptr = {:?}", dynamictop_ptr);
    // debug!("###### dynamictop_ptr_offset = {:?}", dynamictop_ptr_offset);

    let mem = &mut memory[dynamictop_ptr..dynamictop_ptr_offset];
    LittleEndian::write_u32(mem, dynamic_base(STATIC_BUMP));
}

macro_rules! mock_external {
    ($namespace:ident, $name:ident) => {{
        extern "C" fn _mocked_fn() -> i32 {
            debug!("emscripten::{} <mock>", stringify!($name));
            -1
        }

        $namespace.insert(
            stringify!($name),
            Export::Function {
                func: unsafe { FuncPointer::new(_mocked_fn as _) },
                ctx: Context::Internal,
                signature: FuncSig {
                    params: vec![],
                    returns: vec![I32],
                },
            },
        );
    }};
}

macro_rules! func {
    ($namespace:ident, $function:ident) => {{
        unsafe { FuncPointer::new($namespace::$function as _) }
    }};
}

macro_rules! global {
    ($value:expr) => {{
        unsafe {
            GlobalPointer::new(
                // NOTE: Taking a shortcut here. LocalGlobal is a struct containing just u64.
                std::mem::transmute::<&u64, *mut LocalGlobal>(&$value),
            )
        }
    }};
}

pub struct EmscriptenGlobalsData {
    abort: u64,
    // Env namespace
    stacktop: u64,
    stack_max: u64,
    dynamictop_ptr: u64,
    memory_base: u64,
    table_base: u64,
    temp_double_ptr: u64,

    // Global namespace
    infinity: u64,
    nan: u64,
}

pub struct EmscriptenGlobals {
    // The emscripten data
    pub data: EmscriptenGlobalsData,
    // The emscripten memory
    pub memory: LinearMemory,
    pub vm_memory: LocalMemory,
    // The emscripten table
    pub table: TableBacking,
    pub vm_table: LocalTable,
    pub table_min: u32,
    pub table_max: Option<u32>,
    pub memory_min: u32,
    pub memory_max: Option<u32>,
}

impl EmscriptenGlobals {
    pub fn new(
        table_min: u32,
        table_max: Option<u32>,
        memory_min: u32,
        memory_max: Option<u32>,
    ) -> Self {
        // Memory initialization
        let memory_type = Memory {
            min: memory_min,
            max: memory_max,
            shared: false,
        };
        let mut memory = LinearMemory::new(&memory_type);
        let vm_memory = memory.into_vm_memory(LocalMemoryIndex::new(0));

        let table_type = Table {
            ty: ElementType::Anyfunc,
            min: table_min,
            max: table_max,
        };
        let mut table = TableBacking::new(&table_type);
        let vm_table = table.into_vm_table();

        let memory_base = STATIC_BASE as u64;
        let table_base = 0 as u64;
        let temp_double_ptr = 0 as u64;
        let data = EmscriptenGlobalsData {
            abort: 0, // TODO review usage
            // env
            stacktop: stacktop(STATIC_BUMP) as _,
            stack_max: stack_max(STATIC_BUMP) as _,
            dynamictop_ptr: dynamictop_ptr(STATIC_BUMP) as _,
            memory_base: memory_base,
            table_base: table_base,
            temp_double_ptr: temp_double_ptr,

            // global
            infinity: std::f64::INFINITY.to_bits() as _,
            nan: std::f64::NAN.to_bits() as _,
        };

        Self {
            data,
            memory,
            vm_memory,
            table,
            vm_table,
            table_min,
            table_max,
            memory_min,
            memory_max,
        }
    }
}

pub fn generate_emscripten_env(globals: &mut EmscriptenGlobals) -> ImportObject {
    let mut imports = ImportObject::new();
    let mut env_namespace = Namespace::new();
    let mut asm_namespace = Namespace::new();
    let mut global_namespace = Namespace::new();
    let mut global_math_namespace = Namespace::new();

    // Add globals.
    // NOTE: There is really no need for checks, these globals should always be available.

    // We generate a fake Context that traps on access
    let null_ctx = Context::External(ptr::null_mut());

    // Memory
    let local_memory = unsafe { MemoryPointer::new(&mut globals.vm_memory) };

    env_namespace.insert(
        "memory".to_string(),
        Export::Memory {
            local: local_memory,
            ctx: null_ctx,
            memory: Memory {
                min: globals.memory_min,
                max: globals.memory_max,
                shared: false,
            },
        },
    );

    // Table
    let local_table = unsafe { TablePointer::new(&mut globals.vm_table) };

    env_namespace.insert(
        "table".to_string(),
        Export::Table {
            local: local_table,
            // We generate a fake Context that traps on access
            ctx: null_ctx,
            table: Table {
                ty: ElementType::Anyfunc,
                min: globals.table_min,
                max: globals.table_max,
            },
        },
    );

    env_namespace.insert(
        "STACKTOP".to_string(),
        Export::Global {
            local: global!(globals.data.stacktop),
            global: GlobalDesc {
                mutable: false,
                ty: I32,
            },
        },
    );

    env_namespace.insert(
        "STACK_MAX".to_string(),
        Export::Global {
            local: global!(globals.data.stack_max),
            global: GlobalDesc {
                mutable: false,
                ty: I32,
            },
        },
    );

    env_namespace.insert(
        "DYNAMICTOP_PTR".to_string(),
        Export::Global {
            local: global!(globals.data.dynamictop_ptr),
            global: GlobalDesc {
                mutable: false,
                ty: I32,
            },
        },
    );

    env_namespace.insert(
        "tableBase".to_string(),
        Export::Global {
            local: global!(globals.data.table_base),
            global: GlobalDesc {
                mutable: false,
                ty: I32,
            },
        },
    );

    env_namespace.insert(
        "__table_base".to_string(),
        Export::Global {
            local: global!(globals.data.table_base),
            global: GlobalDesc {
                mutable: false,
                ty: I32,
            },
        },
    );

    env_namespace.insert(
        "ABORT".to_string(),
        Export::Global {
            local: global!(globals.data.abort),
            global: GlobalDesc {
                mutable: false,
                ty: I32,
            },
        },
    );

    env_namespace.insert(
        "memoryBase".to_string(),
        Export::Global {
            local: global!(globals.data.memory_base),
            global: GlobalDesc {
                mutable: false,
                ty: I32,
            },
        },
    );

    env_namespace.insert(
        "__memory_base".to_string(),
        Export::Global {
            local: global!(globals.data.memory_base),
            global: GlobalDesc {
                mutable: false,
                ty: I32,
            },
        },
    );

    env_namespace.insert(
        "tempDoublePtr".to_string(),
        Export::Global {
            local: global!(globals.data.temp_double_ptr),
            global: GlobalDesc {
                mutable: false,
                ty: I32,
            },
        },
    );

    global_namespace.insert(
        "Infinity".to_string(),
        Export::Global {
            local: global!(globals.data.infinity),
            global: GlobalDesc {
                mutable: false,
                ty: F64,
            },
        },
    );

    global_namespace.insert(
        "NaN".to_string(),
        Export::Global {
            local: global!(globals.data.nan),
            global: GlobalDesc {
                mutable: false,
                ty: F64,
            },
        },
    );

    // Global Math
    global_math_namespace.insert(
        "pow",
        Export::Function {
            func: func!(math, pow),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![F64, F64],
                returns: vec![F64],
            },
        },
    );

    // Print function
    env_namespace.insert(
        "printf",
        Export::Function {
            func: func!(io, printf),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "putchar",
        Export::Function {
            func: func!(io, putchar),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![],
            },
        },
    );

    // Assert
    env_namespace.insert(
        "___assert_fail",
        Export::Function {
            func: func!(env, ___assert_fail),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32, I32, I32],
                returns: vec![],
            },
        },
    );

    // Lock
    env_namespace.insert(
        "___lock",
        Export::Function {
            func: func!(lock, ___lock),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![],
            },
        },
    );

    env_namespace.insert(
        "___unlock",
        Export::Function {
            func: func!(lock, ___unlock),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![],
            },
        },
    );

    env_namespace.insert(
        "___wait",
        Export::Function {
            func: func!(lock, ___wait),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32, I32, I32],
                returns: vec![],
            },
        },
    );
    // Env
    env_namespace.insert(
        "_getenv",
        Export::Function {
            func: func!(env, _getenv),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_setenv",
        Export::Function {
            func: func!(env, _setenv),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_putenv",
        Export::Function {
            func: func!(env, _putenv),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_unsetenv",
        Export::Function {
            func: func!(env, _unsetenv),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_getpwnam",
        Export::Function {
            func: func!(env, _getpwnam),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_getgrnam",
        Export::Function {
            func: func!(env, _getgrnam),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___buildEnvironment",
        Export::Function {
            func: func!(env, ___build_environment),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![],
            },
        },
    );
    // Errno
    env_namespace.insert(
        "___setErrNo",
        Export::Function {
            func: func!(errno, ___seterrno),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![],
            },
        },
    );
    // Syscalls
    env_namespace.insert(
        "___syscall1",
        Export::Function {
            func: func!(syscalls, ___syscall1),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![],
            },
        },
    );

    env_namespace.insert(
        "___syscall3",
        Export::Function {
            func: func!(syscalls, ___syscall3),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall4",
        Export::Function {
            func: func!(syscalls, ___syscall4),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall5",
        Export::Function {
            func: func!(syscalls, ___syscall5),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall6",
        Export::Function {
            func: func!(syscalls, ___syscall6),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall12",
        Export::Function {
            func: func!(syscalls, ___syscall12),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall20",
        Export::Function {
            func: func!(syscalls, ___syscall20),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall220",
        Export::Function {
            func: func!(syscalls, ___syscall220),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall39",
        Export::Function {
            func: func!(syscalls, ___syscall39),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall40",
        Export::Function {
            func: func!(syscalls, ___syscall40),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall10",
        Export::Function {
            func: func!(syscalls, ___syscall10),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall54",
        Export::Function {
            func: func!(syscalls, ___syscall54),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall57",
        Export::Function {
            func: func!(syscalls, ___syscall57),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall63",
        Export::Function {
            func: func!(syscalls, ___syscall63),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall85",
        Export::Function {
            func: func!(syscalls, ___syscall85),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall64",
        Export::Function {
            func: func!(syscalls, ___syscall64),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall102",
        Export::Function {
            func: func!(syscalls, ___syscall102),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall114",
        Export::Function {
            func: func!(syscalls, ___syscall114),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall122",
        Export::Function {
            func: func!(syscalls, ___syscall122),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall140",
        Export::Function {
            func: func!(syscalls, ___syscall140),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall142",
        Export::Function {
            func: func!(syscalls, ___syscall142),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall145",
        Export::Function {
            func: func!(syscalls, ___syscall145),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall146",
        Export::Function {
            func: func!(syscalls, ___syscall146),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall180",
        Export::Function {
            func: func!(syscalls, ___syscall180),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall181",
        Export::Function {
            func: func!(syscalls, ___syscall181),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall192",
        Export::Function {
            func: func!(syscalls, ___syscall192),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall195",
        Export::Function {
            func: func!(syscalls, ___syscall195),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall197",
        Export::Function {
            func: func!(syscalls, ___syscall197),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall201",
        Export::Function {
            func: func!(syscalls, ___syscall201),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall202",
        Export::Function {
            func: func!(syscalls, ___syscall202),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall212",
        Export::Function {
            func: func!(syscalls, ___syscall212),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall221",
        Export::Function {
            func: func!(syscalls, ___syscall221),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall330",
        Export::Function {
            func: func!(syscalls, ___syscall330),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall340",
        Export::Function {
            func: func!(syscalls, ___syscall340),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );
    // Process
    env_namespace.insert(
        "abort",
        Export::Function {
            func: func!(process, em_abort),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![],
            },
        },
    );

    env_namespace.insert(
        "_abort",
        Export::Function {
            func: func!(process, _abort),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![],
                returns: vec![],
            },
        },
    );

    env_namespace.insert(
        "abortStackOverflow",
        Export::Function {
            func: func!(process, abort_stack_overflow),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![],
            },
        },
    );

    env_namespace.insert(
        "_llvm_trap",
        Export::Function {
            func: func!(process, _llvm_trap),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![],
                returns: vec![],
            },
        },
    );

    env_namespace.insert(
        "_fork",
        Export::Function {
            func: func!(process, _fork),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_exit",
        Export::Function {
            func: func!(process, _exit),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![],
            },
        },
    );

    env_namespace.insert(
        "_system",
        Export::Function {
            func: func!(process, _system),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_popen",
        Export::Function {
            func: func!(process, _popen),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );
    // Signal
    env_namespace.insert(
        "_sigemptyset",
        Export::Function {
            func: func!(signal, _sigemptyset),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_sigaddset",
        Export::Function {
            func: func!(signal, _sigaddset),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_sigprocmask",
        Export::Function {
            func: func!(signal, _sigprocmask),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_sigaction",
        Export::Function {
            func: func!(signal, _sigaction),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_signal",
        Export::Function {
            func: func!(signal, _signal),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );
    // Memory
    env_namespace.insert(
        "abortOnCannotGrowMemory",
        Export::Function {
            func: func!(memory, abort_on_cannot_grow_memory),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_emscripten_memcpy_big",
        Export::Function {
            func: func!(memory, _emscripten_memcpy_big),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "enlargeMemory",
        Export::Function {
            func: func!(memory, enlarge_memory),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "getTotalMemory",
        Export::Function {
            func: func!(memory, get_total_memory),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___map_file",
        Export::Function {
            func: func!(memory, ___map_file),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );
    // Exception
    env_namespace.insert(
        "___cxa_allocate_exception",
        Export::Function {
            func: func!(exception, ___cxa_allocate_exception),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___cxa_allocate_exception",
        Export::Function {
            func: func!(exception, ___cxa_throw),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32, I32],
                returns: vec![],
            },
        },
    );

    env_namespace.insert(
        "___cxa_throw",
        Export::Function {
            func: func!(exception, ___cxa_throw),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32, I32],
                returns: vec![],
            },
        },
    );
    // NullFuncs
    env_namespace.insert(
        "nullFunc_i",
        Export::Function {
            func: func!(nullfunc, nullfunc_i),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![],
            },
        },
    );

    env_namespace.insert(
        "nullFunc_ii",
        Export::Function {
            func: func!(nullfunc, nullfunc_ii),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![],
            },
        },
    );

    env_namespace.insert(
        "nullFunc_iii",
        Export::Function {
            func: func!(nullfunc, nullfunc_iii),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![],
            },
        },
    );

    env_namespace.insert(
        "nullFunc_iiii",
        Export::Function {
            func: func!(nullfunc, nullfunc_iiii),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![],
            },
        },
    );

    env_namespace.insert(
        "nullFunc_iiiii",
        Export::Function {
            func: func!(nullfunc, nullfunc_iiiii),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![],
            },
        },
    );

    env_namespace.insert(
        "nullFunc_iiiiii",
        Export::Function {
            func: func!(nullfunc, nullfunc_iiiiii),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![],
            },
        },
    );

    env_namespace.insert(
        "nullFunc_v",
        Export::Function {
            func: func!(nullfunc, nullfunc_v),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![],
            },
        },
    );

    env_namespace.insert(
        "nullFunc_vi",
        Export::Function {
            func: func!(nullfunc, nullfunc_vi),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![],
            },
        },
    );

    env_namespace.insert(
        "nullFunc_vii",
        Export::Function {
            func: func!(nullfunc, nullfunc_vii),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![],
            },
        },
    );

    env_namespace.insert(
        "nullFunc_viii",
        Export::Function {
            func: func!(nullfunc, nullfunc_viii),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![],
            },
        },
    );

    env_namespace.insert(
        "nullFunc_viiii",
        Export::Function {
            func: func!(nullfunc, nullfunc_viiii),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![],
            },
        },
    );

    env_namespace.insert(
        "nullFunc_viiiii",
        Export::Function {
            func: func!(nullfunc, nullfunc_viiiii),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![],
            },
        },
    );

    env_namespace.insert(
        "nullFunc_viiiiii",
        Export::Function {
            func: func!(nullfunc, nullfunc_viiiiii),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![],
            },
        },
    );
    // Time
    env_namespace.insert(
        "_gettimeofday",
        Export::Function {
            func: func!(time, _gettimeofday),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_clock_gettime",
        Export::Function {
            func: func!(time, _clock_gettime),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___clock_gettime",
        Export::Function {
            func: func!(time, _clock_gettime),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_clock",
        Export::Function {
            func: func!(time, _clock),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_difftime",
        Export::Function {
            func: func!(time, _difftime),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![F64],
            },
        },
    );

    env_namespace.insert(
        "_asctime",
        Export::Function {
            func: func!(time, _asctime),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_asctime_r",
        Export::Function {
            func: func!(time, _asctime_r),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_localtime",
        Export::Function {
            func: func!(time, _localtime),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_time",
        Export::Function {
            func: func!(time, _time),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_strftime",
        Export::Function {
            func: func!(time, _strftime),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32, I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_localtime_r",
        Export::Function {
            func: func!(time, _localtime_r),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_getpagesize",
        Export::Function {
            func: func!(env, _getpagesize),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_sysconf",
        Export::Function {
            func: func!(env, _sysconf),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![I32],
            },
        },
    );

    // Math
    asm_namespace.insert(
        "f64-rem",
        Export::Function {
            func: func!(math, f64_rem),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![F64, F64],
                returns: vec![F64],
            },
        },
    );

    env_namespace.insert(
        "_llvm_log10_f64",
        Export::Function {
            func: func!(math, _llvm_log10_f64),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![F64],
                returns: vec![F64],
            },
        },
    );

    env_namespace.insert(
        "_llvm_log2_f64",
        Export::Function {
            func: func!(math, _llvm_log2_f64),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![F64],
                returns: vec![F64],
            },
        },
    );

    //
    env_namespace.insert(
        "__setjmp",
        Export::Function {
            func: func!(jmp, __setjmp),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "__longjmp",
        Export::Function {
            func: func!(jmp, __longjmp),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![],
            },
        },
    );

    env_namespace.insert(
        "___syscall110",
        Export::Function {
            func: func!(syscalls, ___syscall110),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall15",
        Export::Function {
            func: func!(syscalls, ___syscall15),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall168",
        Export::Function {
            func: func!(syscalls, ___syscall168),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall191",
        Export::Function {
            func: func!(syscalls, ___syscall191),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );
    env_namespace.insert(
        "___syscall194",
        Export::Function {
            func: func!(syscalls, ___syscall194),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );
    env_namespace.insert(
        "___syscall196",
        Export::Function {
            func: func!(syscalls, ___syscall196),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );
    env_namespace.insert(
        "___syscall199",
        Export::Function {
            func: func!(syscalls, ___syscall199),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall268",
        Export::Function {
            func: func!(syscalls, ___syscall268),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall272",
        Export::Function {
            func: func!(syscalls, ___syscall272),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall295",
        Export::Function {
            func: func!(syscalls, ___syscall295),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall300",
        Export::Function {
            func: func!(syscalls, ___syscall300),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall334",
        Export::Function {
            func: func!(syscalls, ___syscall334),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall38",
        Export::Function {
            func: func!(syscalls, ___syscall38),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall60",
        Export::Function {
            func: func!(syscalls, ___syscall60),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall66",
        Export::Function {
            func: func!(syscalls, ___syscall66),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall75",
        Export::Function {
            func: func!(syscalls, ___syscall75),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall91",
        Export::Function {
            func: func!(syscalls, ___syscall91),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "___syscall97",
        Export::Function {
            func: func!(syscalls, ___syscall97),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_endgrent",
        Export::Function {
            func: func!(process, _endgrent),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![],
                returns: vec![],
            },
        },
    );

    env_namespace.insert(
        "_execve",
        Export::Function {
            func: func!(process, _execve),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_getaddrinfo",
        Export::Function {
            func: func!(env, _getaddrinfo),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32, I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_gmtime_r",
        Export::Function {
            func: func!(time, _gmtime_r),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_kill",
        Export::Function {
            func: func!(process, _kill),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_llvm_stackrestore",
        Export::Function {
            func: func!(process, _llvm_stackrestore),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![],
            },
        },
    );

    env_namespace.insert(
        "_mktime",
        Export::Function {
            func: func!(time, _mktime),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_raise",
        Export::Function {
            func: func!(process, _raise),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_sem_init",
        Export::Function {
            func: func!(process, _sem_init),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_sem_post",
        Export::Function {
            func: func!(process, _sem_post),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_sem_wait",
        Export::Function {
            func: func!(process, _sem_wait),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_setgrent",
        Export::Function {
            func: func!(process, _setgrent),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![],
                returns: vec![],
            },
        },
    );

    env_namespace.insert(
        "_setgroups",
        Export::Function {
            func: func!(process, _setgroups),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_setitimer",
        Export::Function {
            func: func!(process, _setitimer),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32, I32],
                returns: vec![I32],
            },
        },
    );

    //
    env_namespace.insert(
        "_sigsuspend",
        Export::Function {
            func: func!(signal, _sigsuspend),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_setitimer",
        Export::Function {
            func: func!(process, _setitimer),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_usleep",
        Export::Function {
            func: func!(process, _usleep),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_utimes",
        Export::Function {
            func: func!(process, _utimes),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_waitpid",
        Export::Function {
            func: func!(process, _waitpid),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_dlclose",
        Export::Function {
            func: func!(linking, _dlclose),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_dlopen",
        Export::Function {
            func: func!(linking, _dlopen),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_dlsym",
        Export::Function {
            func: func!(linking, _dlsym),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![I32, I32],
                returns: vec![I32],
            },
        },
    );

    env_namespace.insert(
        "_llvm_log10_f32",
        Export::Function {
            func: func!(math, _llvm_log10_f32),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![F64],
                returns: vec![F64],
            },
        },
    );

    env_namespace.insert(
        "_llvm_log2_f32",
        Export::Function {
            func: func!(math, _llvm_log2_f32),
            ctx: Context::Internal,
            signature: FuncSig {
                params: vec![F64],
                returns: vec![F64],
            },
        },
    );

    // mock_external!(env_namespace, _time);
    // mock_external!(env_namespace, _sysconf);
    // mock_external!(env_namespace, _strftime);
    // mock_external!(env_namespace, _sigprocmask);
    // mock_external!(env_namespace, _sigemptyset);
    // mock_external!(env_namespace, _sigaddset);
    // mock_external!(env_namespace, _sigaction);

    mock_external!(env_namespace, _sched_yield);
    // mock_external!(env_namespace, _localtime_r);
    // mock_external!(env_namespace, _localtime);
    mock_external!(env_namespace, _llvm_stacksave);
    // mock_external!(env_namespace, _gettimeofday);
    // mock_external!(env_namespace, _getpagesize);
    mock_external!(env_namespace, _getgrent);
    // mock_external!(env_namespace, _fork);
    // mock_external!(env_namespace, _exit);
    // mock_external!(env_namespace, _clock_gettime);
    // mock_external!(env_namespace, ___syscall64);
    // mock_external!(env_namespace, ___syscall63);
    // mock_external!(env_namespace, ___syscall60);
    // mock_external!(env_namespace, ___syscall54);
    // mock_external!(env_namespace, ___syscall39);
    // mock_external!(env_namespace, ___syscall340);
    // mock_external!(env_namespace, ___syscall221);
    // mock_external!(env_namespace, ___syscall212);
    // mock_external!(env_namespace, ___syscall201);
    // mock_external!(env_namespace, ___syscall197);
    // mock_external!(env_namespace, ___syscall195);
    // mock_external!(env_namespace, ___syscall181);
    // mock_external!(env_namespace, ___syscall180);
    // mock_external!(env_namespace, ___syscall146);
    // mock_external!(env_namespace, ___syscall145);
    // mock_external!(env_namespace, ___syscall142);
    // mock_external!(env_namespace, ___syscall140);
    // mock_external!(env_namespace, ___syscall122);
    // mock_external!(env_namespace, ___syscall102);
    // mock_external!(env_namespace, ___syscall20);
    mock_external!(env_namespace, _dlerror);

    imports.register("env", env_namespace);
    imports.register("asm2wasm", asm_namespace);
    imports.register("global", global_namespace);
    imports.register("global.Math", global_math_namespace);

    imports
}

/// The current version of this crate
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
