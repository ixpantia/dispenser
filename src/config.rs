use std::path::PathBuf;

use crate::manifests::DockerWatcher;

#[derive(serde::Deserialize)]
pub struct ContposeConfig {
    pub path: PathBuf,
    image: Vec<Image>,
}

#[derive(serde::Deserialize)]
struct Image {
    name: String,
    tag: String,
}

impl ContposeConfig {
    pub fn init() -> Self {
        use std::io::Read;
        let mut config = String::new();
        std::fs::File::open("contpose.toml")
            .expect("No contpose config")
            .read_to_string(&mut config)
            .unwrap();
        toml::from_str(&config).unwrap()
    }
    pub fn get_watchers(&self) -> Vec<DockerWatcher> {
        self.image
            .iter()
            .map(|image| DockerWatcher::initialize(&image.name, &image.tag))
            .collect()
    }
}
