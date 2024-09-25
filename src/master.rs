use std::{
    path::Path,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
        Arc,
    },
    thread::JoinHandle,
};

pub struct DockerComposeMaster {
    update_msg: Sender<MasterMsg>,
    watcher_thread: Option<JoinHandle<()>>,
    is_done: Arc<AtomicBool>,
}

impl Drop for DockerComposeMaster {
    fn drop(&mut self) {
        // Wait for thread to stop
        self.watcher_thread.take().map(|thread| thread.join());
    }
}

pub enum MasterMsg {
    Update,
    Stop,
}

impl DockerComposeMaster {
    pub fn is_done(&self) -> bool {
        self.is_done.load(Ordering::SeqCst)
    }
    pub fn send_msg(&self, msg: MasterMsg) {
        let _ = self.update_msg.send(msg);
    }
    pub fn initialize(path: impl AsRef<Path>) -> Self {
        let is_done_shared = Arc::new(AtomicBool::new(false));
        let is_done = Arc::clone(&is_done_shared);
        let (update_msg, update_recv) = std::sync::mpsc::channel::<MasterMsg>();
        let path: Box<Path> = path.as_ref().into();
        let watch_fn = move || loop {
            let exit_status = Command::new("docker")
                .arg("compose")
                .arg("up")
                .args(["--pull", "always"])
                .arg("-d")
                .current_dir(&path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            match exit_status {
                Ok(es) if es.success() => log::info!("Updated correctly!"),
                Ok(es) => log::warn!(
                    "Docker compose up not successful exit with code {:?}",
                    es.code()
                ),
                Err(e) => {
                    log::error!("Failed to invoce docker compose: {}", e);
                    std::process::exit(1);
                }
            }

            // Wait for an update msg before restarting the loop
            match update_recv.recv().expect("Broken pipe") {
                MasterMsg::Update => {
                    log::info!("Received updated message!, will start updating soon...");
                }
                MasterMsg::Stop => {
                    log::warn!("Received stop signal");
                    let _ = Command::new("docker")
                        .arg("compose")
                        .arg("down")
                        .current_dir(&path)
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status();
                    log::warn!("Stopped the compose service");
                    is_done_shared.store(true, Ordering::SeqCst);
                    break;
                }
            }
        };
        let watcher_thread = Some(std::thread::spawn(watch_fn));
        DockerComposeMaster {
            watcher_thread,
            update_msg,
            is_done,
        }
    }
}
