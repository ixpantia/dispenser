use std::{
    path::Path,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc::Sender,
        Arc,
    },
    thread::JoinHandle,
};

use crate::config::{DispenserVars, Initialize};

#[derive(Clone, Copy, Eq, PartialEq)]
#[repr(u32)]
enum MasterStatus {
    Stopped = 0,
    Reloading = 1,
    Started = 2,
}

impl MasterStatus {
    #[inline]
    fn from_u32(val: u32) -> MasterStatus {
        match val {
            0 => MasterStatus::Stopped,
            1 => MasterStatus::Reloading,
            2 => MasterStatus::Started,
            _ => panic!("Impossible"),
        }
    }
    #[inline]
    fn into_u32(self) -> u32 {
        self as u32
    }
}

struct AtomicMasterStatus(AtomicU32);

impl AtomicMasterStatus {
    fn new(val: MasterStatus) -> Self {
        AtomicMasterStatus(AtomicU32::new(val as u32))
    }
    fn load(&self, ordering: Ordering) -> MasterStatus {
        MasterStatus::from_u32(self.0.load(ordering))
    }
    fn store(&self, value: MasterStatus, ordering: Ordering) {
        self.0.store(value.into_u32(), ordering)
    }
}

pub struct DockerComposeMaster {
    update_msg: Sender<MasterMsg>,
    watcher_thread: Option<JoinHandle<()>>,
    status: Arc<AtomicMasterStatus>,
}

impl Drop for DockerComposeMaster {
    fn drop(&mut self) {
        // Wait for thread to stop
        self.watcher_thread.take().map(|thread| thread.join());
    }
}

pub enum MasterMsg {
    Detach,
    Update(Action),
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Update,
    Recreate,
}

impl Action {
    fn flags(self) -> &'static [&'static str] {
        match self {
            Action::Update => &[],
            Action::Recreate => &["--force-recreate"],
        }
    }
}

impl DockerComposeMaster {
    pub fn is_stopped(&self) -> bool {
        self.status.load(Ordering::SeqCst) == MasterStatus::Stopped
    }
    pub fn send_msg(&self, msg: MasterMsg) {
        let _ = self.update_msg.send(msg);
    }
    pub fn initialize(path: impl AsRef<Path>, initialize: Initialize, vars: DispenserVars) -> Self {
        let status_shared = Arc::new(AtomicMasterStatus::new(MasterStatus::Stopped));
        let status = Arc::clone(&status_shared);
        let (update_msg, update_recv) = std::sync::mpsc::channel::<MasterMsg>();

        if matches!(initialize, Initialize::Immediately) {
            let _ = update_msg.send(MasterMsg::Update(Action::Recreate));
        }

        let path: Box<Path> = path.as_ref().into();
        let watch_fn = {
            let path = path.clone();
            move || loop {
                // Wait for an update msg before restarting the loop
                match update_recv.recv().expect("Broken pipe") {
                    MasterMsg::Update(action) => {
                        match action {
                            Action::Update => log::info!("Received update directive. Composing the updated services at {path:?}..."),
                            Action::Recreate => log::info!("Received run/restart directive. Recreating the updated services at {path:?}..."),
                        };
                        let exit_status = Command::new("docker")
                            .arg("compose")
                            .arg("up")
                            .args(["--pull", "always"])
                            .args(action.flags())
                            .arg("-d")
                            .current_dir(&path)
                            .envs(vars.inner.iter())
                            .stdin(Stdio::null())
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .status();
                        match exit_status {
                            Ok(es) if es.success() => {
                                log::info!("Services for {path:?} are up and running!");
                                status_shared.store(MasterStatus::Started, Ordering::SeqCst);
                            }
                            Ok(es) => log::warn!(
                                "Docker compose up at {path:?} not successful exit with code {:?}",
                                es.code()
                            ),
                            Err(e) => {
                                log::error!("Failed to invoce docker compose at {path:?}: {}", e);
                            }
                        }
                    }
                    MasterMsg::Stop => {
                        log::warn!("Received stop signal for instace {path:?}");
                        let _ = Command::new("docker")
                            .arg("compose")
                            .arg("stop")
                            .current_dir(&path)
                            .stdin(Stdio::null())
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .status();
                        log::warn!("Stopped the compose service at {path:?}");
                        status_shared.store(MasterStatus::Stopped, Ordering::SeqCst);
                        break;
                    }
                    MasterMsg::Detach => {
                        log::warn!("Detaching from docker compose at {path:?}");
                        status_shared.store(MasterStatus::Stopped, Ordering::SeqCst);
                        break;
                    }
                }
            }
        };
        let watcher_thread = Some(std::thread::spawn(watch_fn));
        DockerComposeMaster {
            watcher_thread,
            update_msg,
            status,
        }
    }
}
