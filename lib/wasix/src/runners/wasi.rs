//! WebC container support for running WASI modules

use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Error};
use tracing::Instrument;
use virtual_fs::{ArcBoxFile, FileSystem, TmpFileSystem, VirtualFile};
use wasmer::Module;
use webc::metadata::{annotations::Wasi, Command};

use crate::{
    bin_factory::BinaryPackage,
    capabilities::Capabilities,
    runners::{wasi_common::CommonWasiOptions, MappedDirectory, MountedDirectory},
    runtime::task_manager::VirtualTaskManagerExt,
    Runtime, WasiEnvBuilder, WasiRuntimeError,
};

use super::wasi_common::MappedCommand;

#[derive(Debug, Default, Clone)]
pub struct WasiRunner {
    wasi: CommonWasiOptions,
    stdin: Option<ArcBoxFile>,
    stdout: Option<ArcBoxFile>,
    stderr: Option<ArcBoxFile>,
}

impl WasiRunner {
    /// Constructs a new `WasiRunner`.
    pub fn new() -> Self {
        WasiRunner::default()
    }

    /// Returns the current arguments for this `WasiRunner`
    pub fn get_args(&self) -> Vec<String> {
        self.wasi.args.clone()
    }

    /// Builder method to provide CLI args to the runner
    pub fn with_args<A, S>(mut self, args: A) -> Self
    where
        A: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.set_args(args);
        self
    }

    /// Set the CLI args
    pub fn set_args<A, S>(&mut self, args: A)
    where
        A: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.wasi.args = args.into_iter().map(|s| s.into()).collect();
    }

    /// Builder method to provide environment variables to the runner.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.set_env(key, value);
        self
    }

    /// Provide environment variables to the runner.
    pub fn set_env(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.wasi.env.insert(key.into(), value.into());
    }

    pub fn with_envs<I, K, V>(mut self, envs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.set_envs(envs);
        self
    }

    pub fn set_envs<I, K, V>(&mut self, envs: I)
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        for (key, value) in envs {
            self.wasi.env.insert(key.into(), value.into());
        }
    }

    pub fn with_forward_host_env(mut self, forward: bool) -> Self {
        self.set_forward_host_env(forward);
        self
    }

    pub fn set_forward_host_env(&mut self, forward: bool) {
        self.wasi.forward_host_env = forward;
    }

    pub fn with_mapped_directories<I, D>(self, dirs: I) -> Self
    where
        I: IntoIterator<Item = D>,
        D: Into<MappedDirectory>,
    {
        self.with_mounted_directories(dirs.into_iter().map(Into::into).map(MountedDirectory::from))
    }

    pub fn with_mounted_directories<I, D>(mut self, dirs: I) -> Self
    where
        I: IntoIterator<Item = D>,
        D: Into<MountedDirectory>,
    {
        self.wasi.mounts.extend(dirs.into_iter().map(Into::into));
        self
    }

    pub fn mount(&mut self, dest: String, fs: Arc<dyn FileSystem + Send + Sync>) -> &mut Self {
        self.wasi.mounts.push(MountedDirectory { guest: dest, fs });
        self
    }

    pub fn set_current_dir(&mut self, dir: impl Into<PathBuf>) {
        self.wasi.current_dir = Some(dir.into());
    }

    pub fn with_current_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.set_current_dir(dir);
        self
    }

    /// Add a package that should be available to the instance at runtime.
    pub fn add_injected_package(&mut self, pkg: BinaryPackage) -> &mut Self {
        self.wasi.injected_packages.push(pkg);
        self
    }

    /// Add a package that should be available to the instance at runtime.
    pub fn with_injected_package(mut self, pkg: BinaryPackage) -> Self {
        self.add_injected_package(pkg);
        self
    }

    /// Add packages that should be available to the instance at runtime.
    pub fn add_injected_packages(
        &mut self,
        packages: impl IntoIterator<Item = BinaryPackage>,
    ) -> &mut Self {
        self.wasi.injected_packages.extend(packages);
        self
    }

    /// Add packages that should be available to the instance at runtime.
    pub fn with_injected_packages(
        mut self,
        packages: impl IntoIterator<Item = BinaryPackage>,
    ) -> Self {
        self.add_injected_packages(packages);
        self
    }

    pub fn add_mapped_host_command(&mut self, alias: impl Into<String>, target: impl Into<String>) {
        self.wasi.mapped_host_commands.push(MappedCommand {
            alias: alias.into(),
            target: target.into(),
        });
    }

    pub fn with_mapped_host_command(
        mut self,
        alias: impl Into<String>,
        target: impl Into<String>,
    ) -> Self {
        self.add_mapped_host_command(alias, target);
        self
    }

    pub fn add_mapped_host_commands(&mut self, commands: impl IntoIterator<Item = MappedCommand>) {
        self.wasi.mapped_host_commands.extend(commands);
    }

    pub fn with_mapped_host_commands(
        mut self,
        commands: impl IntoIterator<Item = MappedCommand>,
    ) -> Self {
        self.add_mapped_host_commands(commands);
        self
    }

    pub fn capabilities_mut(&mut self) -> &mut Capabilities {
        &mut self.wasi.capabilities
    }

    pub fn with_capabilities(mut self, capabilities: Capabilities) -> Self {
        self.set_capabilities(capabilities);
        self
    }

    pub fn set_capabilities(&mut self, capabilities: Capabilities) {
        self.wasi.capabilities = capabilities;
    }

    pub fn with_stdin(mut self, stdin: Box<dyn VirtualFile + Send + Sync>) -> Self {
        self.set_stdin(stdin);
        self
    }

    pub fn set_stdin(&mut self, stdin: Box<dyn VirtualFile + Send + Sync>) -> &mut Self {
        self.stdin = Some(ArcBoxFile::new(stdin));
        self
    }

    pub fn with_stdout(mut self, stdout: Box<dyn VirtualFile + Send + Sync>) -> Self {
        self.set_stdout(stdout);
        self
    }

    pub fn set_stdout(&mut self, stdout: Box<dyn VirtualFile + Send + Sync>) -> &mut Self {
        self.stdout = Some(ArcBoxFile::new(stdout));
        self
    }

    pub fn with_stderr(mut self, stderr: Box<dyn VirtualFile + Send + Sync>) -> Self {
        self.set_stderr(stderr);
        self
    }

    pub fn set_stderr(&mut self, stderr: Box<dyn VirtualFile + Send + Sync>) -> &mut Self {
        self.stderr = Some(ArcBoxFile::new(stderr));
        self
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub(crate) fn prepare_webc_env(
        &self,
        program_name: &str,
        wasi: &Wasi,
        pkg: Option<&BinaryPackage>,
        runtime: Arc<dyn Runtime + Send + Sync>,
        root_fs: Option<TmpFileSystem>,
    ) -> Result<WasiEnvBuilder, anyhow::Error> {
        let mut builder = WasiEnvBuilder::new(program_name).runtime(runtime);

        let container_fs = if let Some(pkg) = pkg {
            builder.add_webc(pkg.clone());
            Some(Arc::clone(&pkg.webc_fs))
        } else {
            None
        };

        self.wasi
            .prepare_webc_env(&mut builder, container_fs, wasi, root_fs)?;

        if let Some(stdin) = &self.stdin {
            builder.set_stdin(Box::new(stdin.clone()));
        }
        if let Some(stdout) = &self.stdout {
            builder.set_stdout(Box::new(stdout.clone()));
        }
        if let Some(stderr) = &self.stderr {
            builder.set_stderr(Box::new(stderr.clone()));
        }

        Ok(builder)
    }

    pub fn run_wasm(
        &self,
        runtime: Arc<dyn Runtime + Send + Sync>,
        program_name: &str,
        module: &Module,
        asyncify: bool,
    ) -> Result<(), Error> {
        let wasi = webc::metadata::annotations::Wasi::new(program_name);
        let mut store = runtime.new_store();
        let env = self.prepare_webc_env(program_name, &wasi, None, runtime, None)?;

        if asyncify {
            env.run_with_store_async(module.clone(), store)?;
        } else {
            env.run_with_store(module.clone(), &mut store)?;
        }

        Ok(())
    }
}

impl crate::runners::Runner for WasiRunner {
    fn can_run_command(command: &Command) -> Result<bool, Error> {
        Ok(command
            .runner
            .starts_with(webc::metadata::annotations::WASI_RUNNER_URI))
    }

    #[tracing::instrument(skip_all)]
    fn run_command(
        &mut self,
        command_name: &str,
        pkg: &BinaryPackage,
        runtime: Arc<dyn Runtime + Send + Sync>,
    ) -> Result<(), Error> {
        let cmd = pkg
            .get_command(command_name)
            .with_context(|| format!("The package doesn't contain a \"{command_name}\" command"))?;
        let wasi = cmd
            .metadata()
            .annotation("wasi")?
            .unwrap_or_else(|| Wasi::new(command_name));

        let env = self
            .prepare_webc_env(command_name, &wasi, Some(pkg), Arc::clone(&runtime), None)
            .context("Unable to prepare the WASI environment")?
            .build()?;

        let store = runtime.new_store();

        let command_name = command_name.to_string();
        let tasks = runtime.task_manager().clone();
        let pkg = pkg.clone();

        let exit_code = tasks.spawn_and_block_on(
            async move {
                let mut task_handle =
                    crate::bin_factory::spawn_exec(pkg, &command_name, store, env, &runtime)
                        .await
                        .context("Spawn failed")?;

                task_handle
                    .wait_finished()
                    .await
                    .map_err(|err| Arc::into_inner(err).expect("Error shouldn't be shared"))
                    .context("Unable to wait for the process to exit")
            }
            .in_current_span(),
        )?;

        if exit_code.raw() == 0 {
            Ok(())
        } else {
            Err(WasiRuntimeError::Wasi(crate::WasiError::Exit(exit_code)).into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_and_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<WasiRunner>();
        assert_sync::<WasiRunner>();
    }
}
