use minijinja::Environment;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path, path::PathBuf};

use crate::secrets;

fn default_gcp_secret_version() -> String {
    "latest".to_string()
}

#[derive(Debug, serde::Deserialize, Clone)]
#[serde(tag = "source", rename_all = "snake_case")]
enum Secret {
    Google {
        name: String,
        #[serde(default = "default_gcp_secret_version")]
        version: String,
    },
}

#[derive(Debug, serde::Deserialize, Clone)]
#[serde(untagged)]
enum ServiceVarEntry {
    Raw(String),
    Secret(Secret),
}

#[derive(Debug, Default, Clone)]
pub struct ServiceVars {
    inner: HashMap<String, ServiceVarEntry>,
}

impl<'de> Deserialize<'de> for ServiceVars {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let inner = HashMap::deserialize(deserializer)?;
        Ok(Self { inner })
    }
}

impl ServiceVars {
    pub async fn materialize(self) -> ServiceVarsMaterialized {
        let mut inner = HashMap::new();
        for (key, entry) in self.inner {
            let value = match entry {
                ServiceVarEntry::Raw(s) => s,
                ServiceVarEntry::Secret(secret) => match secret {
                    Secret::Google { name, version } => {
                        secrets::gcp::fetch_secret(&name, &version).await
                    }
                },
            };
            inner.insert(key, value);
        }
        ServiceVarsMaterialized { inner }
    }
}

#[derive(Debug, Default, Clone)]
pub struct ServiceVarsMaterialized {
    inner: HashMap<String, String>,
}

impl ServiceVarsMaterialized {
    pub async fn try_init() -> Result<Self, ServiceConfigError> {
        let vars_raw = ServiceVars::try_init()?;
        Ok(vars_raw.materialize().await)
    }
}

impl Serialize for ServiceVarsMaterialized {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.inner.serialize(serializer)
    }
}

/// Files that match dispenser.vars | *.dispenser.vars
/// Sorted
fn list_vars_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    let cli_args = crate::cli::get_cli_args();

    let search_dir = cli_args.config.parent().map_or(Path::new("."), |p| {
        if p.as_os_str().is_empty() {
            Path::new(".")
        } else {
            p
        }
    });
    if let Ok(entries) = std::fs::read_dir(search_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() {
                if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
                    if file_name == "dispenser.vars" || file_name.ends_with(".dispenser.vars") {
                        files.push(path);
                    }
                }
            }
        }
    }

    files.sort(); // Sort the paths alphabetically
    files
}

impl ServiceVars {
    fn try_init_from_string(val: &str) -> Result<Self, ServiceConfigError> {
        Ok(toml::from_str(val)?)
    }

    fn combine(vars: Vec<Self>) -> Self {
        let mut combined_inner = HashMap::new();
        vars.into_iter().for_each(|var_set| {
            combined_inner.extend(var_set.inner);
        });
        Self {
            inner: combined_inner,
        }
    }

    fn try_init() -> Result<Self, ServiceConfigError> {
        use std::io::Read;
        let mut vars = Vec::new();
        let vars_files = list_vars_files();
        for vars_file in vars_files {
            match std::fs::File::open(vars_file) {
                Ok(mut file) => {
                    let mut this_vars = String::new();
                    file.read_to_string(&mut this_vars)?;
                    match Self::try_init_from_string(&this_vars) {
                        Ok(this_vars) => vars.push(this_vars),
                        Err(e) => log::error!("Error parsing vars file: {e}"),
                    }
                }
                Err(e) => log::error!("Error reading vars file: {e}"),
            }
        }

        Ok(Self::combine(vars))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ServiceConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("Templating error: {0:?}")]
    Template((PathBuf, minijinja::Error)),
}

pub fn render_template(
    template_str: &str,
    vars: &ServiceVarsMaterialized,
) -> Result<String, minijinja::Error> {
    let mut env = Environment::new();

    let syntax = minijinja::syntax::SyntaxConfig::builder()
        .variable_delimiters("${", "}")
        .build()
        .expect("This really should not fail. If this fail something has gone horribly wrong.");

    env.set_syntax(syntax);
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);

    let template = env.template_from_str(template_str)?;
    Ok(template.render(vars)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vars_parsing() {
        let input = r#"
            var1 = "value1"
            var2 = "value2"
        "#;
        let vars = ServiceVars::try_init_from_string(input).expect("Failed to parse vars");
        let get_val = |k| match vars.inner.get(k) {
            Some(ServiceVarEntry::Raw(s)) => Some(s.as_str()),
            _ => None,
        };
        assert_eq!(get_val("var1"), Some("value1"));
        assert_eq!(get_val("var2"), Some("value2"));
    }

    #[tokio::test]
    async fn test_template_rendering() {
        let mut inner = HashMap::new();
        inner.insert("base_path".to_string(), "/app".to_string());
        inner.insert("version".to_string(), "1.2.3".to_string());

        let vars = ServiceVarsMaterialized { inner };

        let template = "image: myapp:${ version }\npath: ${ base_path }/service";
        let rendered = render_template(template, &vars).expect("Failed to render");

        assert!(rendered.contains("image: myapp:1.2.3"));
        assert!(rendered.contains("path: /app/service"));
    }
}
