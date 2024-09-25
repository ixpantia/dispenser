// cat ~/my_password.txt | docker login --username foo --password-stdin

use std::io::{Read, Write};
use std::process::Stdio;

pub static REGISTRY: std::sync::OnceLock<Box<str>> = std::sync::OnceLock::new();
pub static USER: std::sync::OnceLock<Box<str>> = std::sync::OnceLock::new();
pub static TOKEN: std::sync::OnceLock<Box<str>> = std::sync::OnceLock::new();

pub fn registry() -> &'static str {
    REGISTRY
        .get_or_init(|| std::env::var("CONTPOSE_REGISTRY").unwrap().into())
        .as_ref()
}

pub fn user() -> &'static str {
    USER.get_or_init(|| std::env::var("CONTPOSE_USER").unwrap().into())
        .as_ref()
}

pub fn token() -> &'static str {
    TOKEN
        .get_or_init(|| std::env::var("CONTPOSE_TOKEN").unwrap().into())
        .as_ref()
}

pub fn login() {
    let mut login_process = std::process::Command::new("docker")
        .arg("login")
        .args(["--username", user()])
        .arg("--password-stdin")
        .arg(registry())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Unable to run docker login");

    let mut login_stdin = login_process.stdin.take().expect("Unable to take stdin");
    let mut login_stderr = login_process
        .stderr
        .take()
        .expect("Unable to capture stderr");

    let mut login_stdout = login_process
        .stdout
        .take()
        .expect("Unable to capture stderr");

    std::thread::spawn(move || {
        login_stdin
            .write_all(token().as_bytes())
            .expect("Unable to write password to docker login");
    });

    let mut stdout = String::new();
    let mut stderr = String::new();

    let _ = login_stdout.read_to_string(&mut stdout);
    let _ = login_stderr.read_to_string(&mut stderr);

    match login_process.wait() {
        Ok(es) if es.success() => log::info!("Successfully logged into registry"),
        Ok(_) => {
            log::warn!("Non zero exit status when logging in");
            log::warn!("{stderr}");
            std::process::exit(1);
        }
        Err(err) => {
            log::error!("Error while trying to log in: {}", err);
            log::warn!("{stderr}");
            std::process::exit(1);
        }
    }
}
