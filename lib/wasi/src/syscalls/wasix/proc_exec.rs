use super::*;
use crate::syscalls::*;

/// Replaces the current process with a new process
///
/// ## Parameters
///
/// * `name` - Name of the process to be spawned
/// * `args` - List of the arguments to pass the process
///   (entries are separated by line feeds)
///
/// ## Return
///
/// Returns a bus process id that can be used to invoke calls
pub fn proc_exec<M: MemorySize>(
    mut ctx: FunctionEnvMut<'_, WasiEnv>,
    name: WasmPtr<u8, M>,
    name_len: M::Offset,
    args: WasmPtr<u8, M>,
    args_len: M::Offset,
) -> Result<(), WasiError> {
    let memory = ctx.data().memory_view(&ctx);
    let mut name = name.read_utf8_string(&memory, name_len).map_err(|err| {
        warn!("failed to execve as the name could not be read - {}", err);
        WasiError::Exit(Errno::Fault as ExitCode)
    })?;
    trace!(
        "wasi[{}:{}]::proc_exec (name={})",
        ctx.data().pid(),
        ctx.data().tid(),
        name
    );

    let args = args.read_utf8_string(&memory, args_len).map_err(|err| {
        warn!("failed to execve as the args could not be read - {}", err);
        WasiError::Exit(Errno::Fault as ExitCode)
    })?;
    let args: Vec<_> = args
        .split(&['\n', '\r'])
        .map(|a| a.to_string())
        .filter(|a| a.len() > 0)
        .collect();

    // Convert relative paths into absolute paths
    if name.starts_with("./") {
        name = ctx.data().state.fs.relative_path_to_absolute(name);
        trace!(
            "wasi[{}:{}]::rel_to_abs (name={}))",
            ctx.data().pid(),
            ctx.data().tid(),
            name
        );
    }

    // Convert the preopen directories
    let preopen = ctx.data().state.preopen.clone();

    // Get the current working directory
    let (_, cur_dir) = {
        let (memory, state, mut inodes) =
            ctx.data().get_memory_and_wasi_state_and_inodes_mut(&ctx, 0);
        match state
            .fs
            .get_current_dir(inodes.deref_mut(), crate::VIRTUAL_ROOT_FD)
        {
            Ok(a) => a,
            Err(err) => {
                warn!("failed to create subprocess for fork - {}", err);
                return Err(WasiError::Exit(Errno::Fault as ExitCode));
            }
        }
    };

    // Build a new store that will be passed to the thread
    #[cfg(feature = "compiler")]
    let engine = ctx.as_store_ref().engine().clone();
    #[cfg(feature = "compiler")]
    let new_store = Store::new(engine);
    #[cfg(not(feature = "compiler"))]
    let new_store = Store::default();

    // If we are in a vfork we need to first spawn a subprocess of this type
    // with the forked WasiEnv, then do a longjmp back to the vfork point.
    if let Some(mut vfork) = ctx.data_mut().vfork.take() {
        // We will need the child pid later
        let child_pid = ctx.data().process.pid();

        // Restore the WasiEnv to the point when we vforked
        std::mem::swap(&mut vfork.env.inner, &mut ctx.data_mut().inner);
        std::mem::swap(vfork.env.as_mut(), ctx.data_mut());
        let mut wasi_env = *vfork.env;
        wasi_env.owned_handles.push(vfork.handle);
        _prepare_wasi(&mut wasi_env, Some(args));

        // Recrod the stack offsets before we give up ownership of the wasi_env
        let stack_base = wasi_env.stack_base;
        let stack_start = wasi_env.stack_start;

        // Spawn a new process with this current execution environment
        let mut err_exit_code = -2i32 as u32;
        let bus = ctx.data().bus();
        let mut process = __asyncify(&mut ctx, None, move |_| async move {
            Ok(bus
                .spawn(wasi_env)
                .spawn(
                    Some(&ctx),
                    name.as_str(),
                    new_store,
                    &ctx.data().bin_factory,
                )
                .await
                .map_err(|err| {
                    err_exit_code = conv_bus_err_to_exit_code(err);
                    warn!(
                        "failed to execve as the process could not be spawned (vfork) - {}",
                        err
                    );
                    let _ = stderr_write(
                        &ctx,
                        format!("wasm execute failed [{}] - {}\n", name.as_str(), err).as_bytes(),
                    );
                    err
                })
                .ok())
        });

        // If no process was created then we create a dummy one so that an
        // exit code can be processed
        let process = match process {
            Ok(Some(a)) => a,
            _ => {
                debug!(
                    "wasi[{}:{}]::process failed with (err={})",
                    ctx.data().pid(),
                    ctx.data().tid(),
                    err_exit_code
                );
                BusSpawnedProcess::exited_process(err_exit_code)
            }
        };

        // Add the process to the environment state
        {
            trace!(
                "wasi[{}:{}]::spawned sub-process (pid={})",
                ctx.data().pid(),
                ctx.data().tid(),
                child_pid.raw()
            );
            let mut inner = ctx.data().process.write();
            inner
                .bus_processes
                .insert(child_pid.into(), Box::new(process));
        }

        let mut memory_stack = vfork.memory_stack;
        let rewind_stack = vfork.rewind_stack;
        let store_data = vfork.store_data;

        // If the return value offset is within the memory stack then we need
        // to update it here rather than in the real memory
        let pid_offset: u64 = vfork.pid_offset.into();
        if pid_offset >= stack_start && pid_offset < stack_base {
            // Make sure its within the "active" part of the memory stack
            let offset = stack_base - pid_offset;
            if offset as usize > memory_stack.len() {
                warn!("vfork failed - the return value (pid) is outside of the active part of the memory stack ({} vs {})", offset, memory_stack.len());
            } else {
                // Update the memory stack with the new PID
                let val_bytes = child_pid.raw().to_ne_bytes();
                let pstart = memory_stack.len() - offset as usize;
                let pend = pstart + val_bytes.len();
                let pbytes = &mut memory_stack[pstart..pend];
                pbytes.clone_from_slice(&val_bytes);
            }
        } else {
            warn!("vfork failed - the return value (pid) is not being returned on the stack - which is not supported");
        }

        // Jump back to the vfork point and current on execution
        unwind::<M, _>(ctx, move |mut ctx, _, _| {
            // Rewind the stack
            match rewind::<M>(
                ctx,
                memory_stack.freeze(),
                rewind_stack.freeze(),
                store_data,
            ) {
                Errno::Success => OnCalledAction::InvokeAgain,
                err => {
                    warn!("fork failed - could not rewind the stack - errno={}", err);
                    OnCalledAction::Trap(Box::new(WasiError::Exit(Errno::Fault as u32)))
                }
            }
        })?;
        return Ok(());
    }
    // Otherwise we need to unwind the stack to get out of the current executing
    // callstack, steal the memory/WasiEnv and switch it over to a new thread
    // on the new module
    else {
        // We need to unwind out of this process and launch a new process in its place
        unwind::<M, _>(ctx, move |mut ctx, _, _| {
            // Grab a reference to the bus
            let bus = ctx.data().bus().clone();

            // Prepare the environment
            let mut wasi_env = ctx.data_mut().clone();
            _prepare_wasi(&mut wasi_env, Some(args));

            // Get a reference to the runtime
            let bin_factory = ctx.data().bin_factory.clone();
            let tasks = wasi_env.tasks.clone();

            // Create the process and drop the context
            let builder = ctx.data().bus().spawn(wasi_env);

            // Spawn a new process with this current execution environment
            //let pid = wasi_env.process.pid();
            let process = __asyncify(&mut ctx, None, move |_| async move {
                Ok(builder
                    .spawn(Some(&ctx), name.as_str(), new_store, &bin_factory)
                    .await)
            });
            match process {
                Ok(Ok(mut process)) => {
                    // Wait for the sub-process to exit itself - then we will exit
                    let (tx, rx) = std::sync::mpsc::channel();
                    let tasks_inner = tasks.clone();
                    tasks.block_on(Box::pin(async move {
                        loop {
                            tasks_inner.sleep_now(current_caller_id(), 5).await;
                            if let Some(exit_code) = process.inst.exit_code() {
                                tx.send(exit_code).unwrap();
                                break;
                            }
                        }
                    }));
                    let exit_code = rx.recv().unwrap();
                    return OnCalledAction::Trap(Box::new(WasiError::Exit(exit_code as ExitCode)));
                }
                Ok(Err(err)) => {
                    warn!(
                        "failed to execve as the process could not be spawned (fork)[0] - {}",
                        err
                    );
                    OnCalledAction::Trap(Box::new(WasiError::Exit(Errno::Noexec as ExitCode)))
                }
                Err(err) => {
                    warn!(
                        "failed to execve as the process could not be spawned (fork)[1] - {}",
                        err
                    );
                    OnCalledAction::Trap(Box::new(WasiError::Exit(Errno::Noexec as ExitCode)))
                }
            }
        })?;
    }

    // Success
    Ok(())
}