use base64::Engine;
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;

#[derive(serde::Deserialize, Debug)]
struct AuthEntry {
    auth: Option<String>,
}

#[derive(serde::Deserialize, Debug)]
struct DockerConfig {
    auths: Option<HashMap<String, AuthEntry>>,
    #[serde(rename = "credsHelpers")]
    creds_helpers: Option<HashMap<String, String>>,
    #[serde(rename = "credsStore")]
    creds_store: Option<String>,
}

fn get_docker_config_path() -> Result<PathBuf, Box<dyn Error>> {
    let home_dir = env::var("HOME")?;
    let docker_config_path = PathBuf::from(home_dir).join(".docker").join("config.json");
    Ok(docker_config_path)
}

fn read_docker_config() -> Result<DockerConfig, Box<dyn Error>> {
    let docker_config_path = get_docker_config_path()?;
    let file = BufReader::new(File::open(docker_config_path)?);
    let config: DockerConfig = serde_json::from_reader(file)?;
    Ok(config)
}

#[derive(serde::Deserialize)]
struct CredStoreOutput {
    #[serde(rename = "Username")]
    username: String,
    #[serde(rename = "Secret")]
    secret: String,
}

fn call_credential_helper(
    helper: &str,
    registry: &str,
) -> Result<(String, String), Box<dyn Error>> {
    let command = format!("docker-credential-{}", helper);
    let mut process = Command::new(command)
        .arg("get")
        .stderr(Stdio::piped())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let mut stdin = process.stdin.take().unwrap();
    stdin.write_all(registry.as_bytes())?;
    drop(stdin);

    let output = process.wait_with_output()?;
    let output_str = String::from_utf8(output.stdout)?;

    let creds: CredStoreOutput = serde_json::from_str(&output_str)?;
    let username = urlencoding::encode(&creds.username).to_string();
    let secret = urlencoding::encode(&creds.secret).to_string();

    Ok((username, secret))
}

fn decode_auth(auth: &str) -> Result<(String, String), Box<dyn Error>> {
    let decoded = base64::prelude::BASE64_STANDARD.decode(auth)?;
    let decoded_str = String::from_utf8(decoded)?;
    let parts: Vec<&str> = decoded_str.split(':').collect();

    if parts.len() != 2 {
        return Err("Invalid auth format".into());
    }

    Ok((parts[0].to_string(), parts[1].to_string()))
}

pub fn get_docker_credentials_internal(registry: &str) -> Result<(String, String), Box<dyn Error>> {
    let config = read_docker_config()?;

    if let Some(cred_helpers) = config.creds_helpers {
        if let Some(helper) = cred_helpers.get(registry) {
            let helper_str = helper.as_str();
            return call_credential_helper(helper_str, registry);
        }
    }

    if let Some(helper) = config.creds_store {
        let helper_str = helper.as_str();
        return call_credential_helper(helper_str, registry);
    }

    // Fallback to plain text credentials from "auths"
    if let Some(auths) = config.auths {
        if let Some(auth_entry) = auths.get(registry) {
            if let Some(auth) = auth_entry.auth.as_ref() {
                return decode_auth(auth);
            }
        }
    }

    Err("No credentials found".into())
}

pub fn get_docker_credentials(registry: &str) -> (String, String) {
    match get_docker_credentials_internal(registry) {
        Ok(creds) => creds,
        Err(e) => {
            log::error!("Error retrieving credentails for {registry}: {e:?}");
            std::process::exit(1);
        }
    }
}
