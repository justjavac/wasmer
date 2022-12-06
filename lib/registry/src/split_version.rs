use std::fmt;
use core::str::FromStr;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SplitVersion {
    pub original: String,
    pub registry: Option<String>,
    pub package: String,
    pub version: Option<String>,
    pub command: Option<String>,
}

impl fmt::Display for SplitVersion {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let version = self.version.as_deref().unwrap_or("latest");
        let command = self
            .command
            .as_ref()
            .map(|s| format!(":{s}"))
            .unwrap_or_default();
        write!(f, "{}@{version}{command}", self.package)
    }
}

#[test]
fn test_split_version() {
    assert_eq!(
        SplitVersion::parse("registry.wapm.io/graphql/python/python").unwrap(),
        SplitVersion {
            original: "registry.wapm.io/graphql/python/python".to_string(),
            registry: Some("https://registry.wapm.io/graphql".to_string()),
            package: "python/python".to_string(),
            version: None,
            command: None,
        }
    );
    assert_eq!(
        SplitVersion::parse("registry.wapm.io/python/python").unwrap(),
        SplitVersion {
            original: "registry.wapm.io/python/python".to_string(),
            registry: Some("https://registry.wapm.io/graphql".to_string()),
            package: "python/python".to_string(),
            version: None,
            command: None,
        }
    );
    assert_eq!(
        SplitVersion::parse("namespace/name@version:command").unwrap(),
        SplitVersion {
            original: "namespace/name@version:command".to_string(),
            registry: None,
            package: "namespace/name".to_string(),
            version: Some("version".to_string()),
            command: Some("command".to_string()),
        }
    );
    assert_eq!(
        SplitVersion::parse("namespace/name@version").unwrap(),
        SplitVersion {
            original: "namespace/name@version".to_string(),
            registry: None,
            package: "namespace/name".to_string(),
            version: Some("version".to_string()),
            command: None,
        }
    );
    assert_eq!(
        SplitVersion::parse("namespace/name").unwrap(),
        SplitVersion {
            original: "namespace/name".to_string(),
            registry: None,
            package: "namespace/name".to_string(),
            version: None,
            command: None,
        }
    );
    assert_eq!(
        SplitVersion::parse("registry.wapm.io/namespace/name").unwrap(),
        SplitVersion {
            original: "registry.wapm.io/namespace/name".to_string(),
            registry: Some("https://registry.wapm.io/graphql".to_string()),
            package: "namespace/name".to_string(),
            version: None,
            command: None,
        }
    );
    assert_eq!(
        format!("{}", SplitVersion::parse("namespace").unwrap_err()),
        "Invalid package version: \"namespace\"".to_string(),
    );
}

impl SplitVersion {
    pub fn parse(s: &str) -> Result<SplitVersion, anyhow::Error> {
        s.parse()
    }
}

impl FromStr for SplitVersion {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let command = WasmerCLIOptions::command();
        let mut prohibited_package_names = command.get_subcommands().map(|s| s.get_name());

        let re1 = regex::Regex::new(r#"(.*)/(.*)@(.*):(.*)"#).unwrap();
        let re2 = regex::Regex::new(r#"(.*)/(.*)@(.*)"#).unwrap();
        let re3 = regex::Regex::new(r#"(.*)/(.*)"#).unwrap();
        let re4 = regex::Regex::new(r#"(.*)/(.*):(.*)"#).unwrap();

        let mut no_version = false;

        let captures = if re1.is_match(s) {
            re1.captures(s)
                .map(|c| {
                    c.iter()
                        .flatten()
                        .map(|m| m.as_str().to_owned())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        } else if re2.is_match(s) {
            re2.captures(s)
                .map(|c| {
                    c.iter()
                        .flatten()
                        .map(|m| m.as_str().to_owned())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        } else if re4.is_match(s) {
            no_version = true;
            re4.captures(s)
                .map(|c| {
                    c.iter()
                        .flatten()
                        .map(|m| m.as_str().to_owned())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        } else if re3.is_match(s) {
            re3.captures(s)
                .map(|c| {
                    c.iter()
                        .flatten()
                        .map(|m| m.as_str().to_owned())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        } else {
            return Err(anyhow::anyhow!("Invalid package version: {s:?}"));
        };

        let mut namespace = match captures.get(1).cloned() {
            Some(s) => s,
            None => {
                return Err(anyhow::anyhow!(
                    "Invalid package version: {s:?}: no namespace"
                ))
            }
        };

        let name = match captures.get(2).cloned() {
            Some(s) => s,
            None => return Err(anyhow::anyhow!("Invalid package version: {s:?}: no name")),
        };

        let mut registry = None;
        if namespace.contains('/') {
            let (r, n) = namespace.rsplit_once('/').unwrap();
            let mut real_registry = r.to_string();
            if !real_registry.ends_with("graphql") {
                real_registry = format!("{real_registry}/graphql");
            }
            if !real_registry.contains("://") {
                real_registry = format!("https://{real_registry}");
            }
            registry = Some(real_registry);
            namespace = n.to_string();
        }

        let sv = SplitVersion {
            original: s.to_string(),
            registry,
            package: format!("{namespace}/{name}"),
            version: if no_version {
                None
            } else {
                captures.get(3).cloned()
            },
            command: captures.get(if no_version { 3 } else { 4 }).cloned(),
        };

        let svp = sv.package.clone();
        anyhow::ensure!(
            !prohibited_package_names.any(|s| s == sv.package.trim()),
            "Invalid package name {svp:?}"
        );

        Ok(sv)
    }
}