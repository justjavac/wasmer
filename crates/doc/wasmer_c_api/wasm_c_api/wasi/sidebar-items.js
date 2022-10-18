initSidebarItems({"enum":[["wasi_version_t","The version of WASI. This is determined by the imports namespace string."]],"fn":[["wasi_config_arg",""],["wasi_config_capture_stderr",""],["wasi_config_capture_stdin",""],["wasi_config_capture_stdout",""],["wasi_config_env",""],["wasi_config_inherit_stderr",""],["wasi_config_inherit_stdin",""],["wasi_config_inherit_stdout",""],["wasi_config_mapdir",""],["wasi_config_new",""],["wasi_config_overwrite_stderr",""],["wasi_config_overwrite_stdin",""],["wasi_config_overwrite_stdout",""],["wasi_config_preopen_dir",""],["wasi_env_delete","Delete a [`wasi_env_t`]."],["wasi_env_initialize_instance",""],["wasi_env_new","Create a new WASI environment."],["wasi_env_read_stderr",""],["wasi_env_read_stdout",""],["wasi_env_set_memory","Set the memory on a [`wasi_env_t`]."],["wasi_get_imports","Non-standard function to get the imports needed for the WASI implementation ordered as expected by the `wasm_module_t`."],["wasi_get_start_function",""],["wasi_get_wasi_version",""],["wasi_pipe_delete",""],["wasi_pipe_delete_str",""],["wasi_pipe_flush",""],["wasi_pipe_new","Creates a new `wasi_pipe_t` which uses a memory buffer for backing stdin / stdout / stderr"],["wasi_pipe_new_blocking","Same as `wasi_pipe_new`, but the pipe will block to wait for stdin input"],["wasi_pipe_new_internal",""],["wasi_pipe_new_null","Creates a `wasi_pipe_t` callback object that does nothing and redirects stdout / stderr to /dev/null"],["wasi_pipe_read_bytes",""],["wasi_pipe_read_str",""],["wasi_pipe_seek",""],["wasi_pipe_write_bytes",""],["wasi_pipe_write_str",""]],"struct":[["wasi_config_t",""],["wasi_env_t",""],["wasi_pipe_t","The console override is a custom context consisting of callback pointers (which are activated whenever some console I/O occurs) and a “context”, which can be owned or referenced from C. This struct can be used in `wasi_config_overwrite_stdin`, `wasi_config_overwrite_stdout` or `wasi_config_overwrite_stderr` to redirect the output or insert input into the console I/O log."]],"type":[["WasiConsoleIoEnvDestructor",""],["WasiConsoleIoReadCallback","Function callback that takes:"],["WasiConsoleIoSeekCallback",""],["WasiConsoleIoWriteCallback",""]]});