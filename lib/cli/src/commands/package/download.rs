use std::{io::Write, path::PathBuf};

use anyhow::Context;
use futures::StreamExt;
use wasmer_registry::wasmer_env::WasmerEnv;
use wasmer_wasix::runtime::resolver::PackageSpecifier;

/// Download a package from the registry.
#[derive(clap::Parser, Debug)]
pub struct PackageDownload {
    #[clap(flatten)]
    env: WasmerEnv,

    /// Verify that the downloaded file is a valid package.
    #[clap(long)]
    validate: bool,

    /// Path where the package file should be written to.
    /// If not specified, the data will be written to stdout.
    #[clap(short = 'o', long)]
    out_path: PathBuf,

    /// The package to download.
    /// Can be:
    /// * a pakage specifier: `namespace/package[@vesion]`
    /// * a URL
    package: PackageSpecifier,
}

impl PackageDownload {
    pub(crate) fn execute(&self) -> Result<(), anyhow::Error> {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(self.run())
    }

    async fn run(&self) -> Result<(), anyhow::Error> {
        if let Some(parent) = self.out_path.parent() {
            match parent.metadata() {
                Ok(m) => {
                    if !m.is_dir() {
                        anyhow::bail!(
                            "parent of output file is not a directory: '{}'",
                            parent.display()
                        );
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    std::fs::create_dir_all(parent)
                        .context("could not create parent directory of output file")?;
                }
                Err(err) => return Err(err.into()),
            }
        };

        let (full_name, version, api_endpoint, token) = match &self.package {
            PackageSpecifier::Registry { full_name, version } => {
                let endpoint = self.env.registry_endpoint()?;
                let version = version.to_string();
                let version = if version == "*" { None } else { Some(version) };

                (
                    full_name,
                    version,
                    endpoint,
                    self.env.get_token_opt().map(|x| x.to_string()),
                )
            }
            PackageSpecifier::Url(url) => {
                bail!("cannot download a package from a URL: '{}'", url);
            }
            PackageSpecifier::Path(_) => {
                anyhow::bail!("cannot download a package from a local path");
            }
        };

        let package = wasmer_registry::query_package_from_registry(
            api_endpoint.as_str(),
            full_name,
            version.as_deref(),
            token.as_deref(),
        )
        .with_context(|| {
            format!(
                "could not retrieve package information for package '{}' from registry '{}'",
                full_name, api_endpoint,
            )
        })?;

        let download_url = package
            .pirita_url
            .context("registry does provide a container download container download URL")?;

        let client = reqwest::Client::new();
        let mut b = client
            .get(&download_url)
            .header(http::header::ACCEPT, "application/webc");
        if let Some(token) = token {
            b = b.header(http::header::AUTHORIZATION, format!("Bearer {token}"));
        };

        eprintln!("Downloading package...");

        let res = b
            .send()
            .await
            .context("http request failed")?
            .error_for_status()
            .context("http request failed with non-success status code")?;

        let mut tmpfile = tempfile::NamedTempFile::new()?;

        let ty = res
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|t| t.to_str().ok())
            .unwrap_or_default();
        if !(ty == "application/webc" || ty == "application/octet-stream") {
            eprintln!(
                "Warning: response has invalid content type - expected \
                'application/webc' or 'application/octet-stream', got {ty}"
            );
        }

        let mut body = res.bytes_stream();

        while let Some(res) = body.next().await {
            let chunk = res.context("could not read response body")?;
            // Yes, we are mixing async and sync code here, but since this is
            // a top-level command, this can't interfere with other tasks.
            tmpfile
                .write_all(&chunk)
                .context("could not write to temporary file")?;
        }

        tmpfile.as_file_mut().sync_all()?;

        if self.validate {
            eprintln!("Validating package...");
            webc::compat::Container::from_disk(tmpfile.path())
                .context("could not parse downloaded file as a package - invalid download?")?;
            eprintln!("Downloaded package is valid!");
        }

        tmpfile.persist(&self.out_path).with_context(|| {
            format!(
                "could not persist temporary file to '{}'",
                self.out_path.display()
            )
        })?;

        eprintln!("Package downloaded to '{}'", self.out_path.display());

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use wasmer_registry::wasmer_env::WASMER_DIR;

    use super::*;

    /// Download a package from the dev registry.
    #[test]
    fn test_cmd_package_download() {
        let dir = tempfile::tempdir().unwrap();

        let out_path = dir.path().join("hello.webc");

        let cmd = PackageDownload {
            env: WasmerEnv::new(WASMER_DIR.clone(), Some("wasmer.wtf".into()), None, None),
            validate: true,
            out_path: out_path.clone(),
            package: "wasmer/hello@0.1.0".parse().unwrap(),
        };

        cmd.execute().unwrap();

        webc::compat::Container::from_disk(out_path).unwrap();
    }
}