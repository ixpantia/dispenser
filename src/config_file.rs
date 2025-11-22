use minijinja::Environment;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use std::{collections::HashMap, num::NonZeroU64, path::PathBuf};

use cron::Schedule;

#[derive(Debug, Default)]
pub struct DispenserVars {
    inner: HashMap<String, String>,
}

impl<'de> Deserialize<'de> for DispenserVars {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let inner = HashMap::deserialize(deserializer)?;
        Ok(Self { inner })
    }
}

impl Serialize for DispenserVars {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.inner.serialize(serializer)
    }
}

impl DispenserVars {
    fn try_init_from_string(val: &str) -> Result<Self, DispenserConfigError> {
        Ok(toml::from_str(val)?)
    }
    fn try_init() -> Result<Self, DispenserConfigError> {
        use std::io::Read;
        match std::fs::File::open(&crate::cli::get_cli_args().vars) {
            Ok(mut file) => {
                let mut vars = String::new();
                file.read_to_string(&mut vars)?;
                Self::try_init_from_string(&vars)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct DispenserConfigFile {
    pub delay: NonZeroU64,
    #[serde(default)]
    pub instance: Vec<DispenserInstanceConfigEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum DispenserConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("Templating error: {0:?}")]
    Template(#[from] minijinja::Error),
}

impl DispenserConfigFile {
    fn try_init_from_string(
        mut config: String,
        vars: &DispenserVars,
    ) -> Result<Self, DispenserConfigError> {
        let mut env = Environment::new();
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
        let template = env.template_from_str(&config)?;
        config = template.render(&vars)?;

        Ok(toml::from_str(&config)?)
    }
    pub fn try_init() -> Result<Self, DispenserConfigError> {
        use std::io::Read;
        let mut config = String::new();
        std::fs::File::open(&crate::cli::get_cli_args().config)?.read_to_string(&mut config)?;
        // Use handle vars to replace strings with handlevars
        let vars = DispenserVars::try_init()?;

        Self::try_init_from_string(config, &vars)
    }
}

/// Defines when a service should be initialized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Default)]
pub enum Initialize {
    /// The service is started as soon as the application starts.
    #[serde(alias = "immediately", alias = "Immediately")]
    #[default]
    Immediately,
    /// The service is started only when a trigger occurs (e.g., a cron schedule or a detected image update).
    #[serde(
        alias = "on-trigger",
        alias = "OnTrigger",
        alias = "on_trigger",
        alias = "on trigger"
    )]
    OnTrigger,
}

impl From<Initialize> for crate::config::Initialize {
    fn from(value: Initialize) -> Self {
        match value {
            Initialize::Immediately => crate::config::Initialize::Immediately,
            Initialize::OnTrigger => crate::config::Initialize::OnTrigger,
        }
    }
}

#[derive(Debug, serde::Deserialize, Clone)]
pub struct DispenserInstanceConfigEntry {
    pub path: PathBuf,
    #[serde(default)]
    images: Vec<Image>,
    #[serde(default)]
    pub cron: Option<Schedule>,
    /// Defines when the service should be initialized.
    ///
    /// - `Immediately` (default): The service is started as soon as the application starts.
    /// - `OnTrigger`: The service is started only when a trigger occurs (e.g., a cron schedule or a detected image update).
    #[serde(default)]
    pub initialize: Initialize,
}

#[derive(Debug, serde::Deserialize, Clone)]
struct Image {
    registry: String,
    name: String,
    tag: String,
}

impl DispenserConfigFile {
    pub fn into_config(self) -> crate::config::ContposeConfig {
        let instances = self
            .instance
            .into_par_iter()
            .map(|instance| crate::config::ContposeInstanceConfig {
                path: instance.path,
                images: instance
                    .images
                    .into_iter()
                    .map(|image| crate::config::Image {
                        registry: image.registry,
                        name: image.name,
                        tag: image.tag,
                    })
                    .collect(),
                cron: instance.cron,
                initialize: instance.initialize.into(),
            })
            .collect();

        crate::config::ContposeConfig {
            delay: self.delay,
            instances,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_vars_parsing() {
        let input = r#"
            var1 = "value1"
            var2 = "value2"
        "#;
        let vars = DispenserVars::try_init_from_string(input).expect("Failed to parse vars");
        assert_eq!(vars.inner.get("var1").map(|s| s.as_str()), Some("value1"));
        assert_eq!(vars.inner.get("var2").map(|s| s.as_str()), Some("value2"));
    }

    #[test]
    fn test_config_loading_with_templates() {
        let vars_input = r#"
            delay_ms = "500"
            base_path = "/app"
            img_version = "1.2.3"
        "#;
        let vars = DispenserVars::try_init_from_string(vars_input).unwrap();

        let config_input = r#"
            delay = {{ delay_ms }}
            [[instance]]
            path = "{{ base_path }}/service"
            initialize = "on-trigger"

            [[instance.images]]
            registry = "hub"
            name = "service"
            tag = "{{ img_version }}"
        "#;

        let config = DispenserConfigFile::try_init_from_string(config_input.to_string(), &vars)
            .expect("Failed to parse config");

        assert_eq!(config.delay.get(), 500);
        assert_eq!(config.instance.len(), 1);

        let instance = &config.instance[0];
        assert_eq!(instance.path.to_str(), Some("/app/service"));
        assert_eq!(instance.initialize, Initialize::OnTrigger);

        assert_eq!(instance.images.len(), 1);
        assert_eq!(instance.images[0].tag, "1.2.3");
    }

    #[test]
    fn test_initialization_modes() {
        let vars = DispenserVars {
            inner: HashMap::new(),
        };

        // Test default
        let default_config = r#"
            delay = 1
            [[instance]]
            path = "."
        "#;
        let cfg =
            DispenserConfigFile::try_init_from_string(default_config.to_string(), &vars).unwrap();
        assert_eq!(cfg.instance[0].initialize, Initialize::Immediately);

        // Test aliases
        let aliases = vec![
            ("immediately", Initialize::Immediately),
            ("Immediately", Initialize::Immediately),
            ("on-trigger", Initialize::OnTrigger),
            ("OnTrigger", Initialize::OnTrigger),
            ("on_trigger", Initialize::OnTrigger),
            ("on trigger", Initialize::OnTrigger),
        ];

        for (alias, expected) in aliases {
            let toml = format!(
                r#"
                delay = 1
                [[instance]]
                path = "."
                initialize = "{}"
            "#,
                alias
            );
            let cfg = DispenserConfigFile::try_init_from_string(toml, &vars).unwrap();
            assert_eq!(cfg.instance[0].initialize, expected);
        }
    }

    #[test]
    fn test_template_failure() {
        let vars = DispenserVars {
            inner: HashMap::new(),
        };
        let config = r#"
            delay = 1
            [[instance]]
            path = "{{ non_existent }}"
        "#;
        let res = DispenserConfigFile::try_init_from_string(config.to_string(), &vars);
        assert!(
            matches!(res, Err(DispenserConfigError::Template(_))),
            "{:?}",
            res
        );
    }
}
