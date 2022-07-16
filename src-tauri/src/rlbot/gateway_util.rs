use crate::ccprintln;
use sysinfo::{ProcessExt, ProcessRefreshKind, RefreshKind, System, SystemExt};
use tauri::Window;

pub const IDEAL_RLBOT_PORT: u16 = 23233;
const EXECUTABLE_NAME: &str = if cfg!(windows) {
    "RLBot.exe"
} else if cfg!(target_os = "macos") {
    "RLBot_mac"
} else {
    "RLBot"
};

pub fn find_existing_process(window: &Window) -> Option<u16> {
    let system = System::new_with_specifics(RefreshKind::new().with_processes(ProcessRefreshKind::new()));

    if let Some(process_info) = system.processes_by_name(EXECUTABLE_NAME).next() {
        if process_info.cmd().len() > 1 {
            let port = process_info.cmd()[1].parse::<u16>().unwrap();
            ccprintln(window, format!("Found existing RLBot process listening on port {}", port));
            return Some(port);
        }
    }

    ccprintln(window, "No existing RLBot process found...".to_owned());
    None
}

pub fn kill_existing_processes(window: &Window) {
    let system = System::new_with_specifics(RefreshKind::new().with_processes(ProcessRefreshKind::new()));

    let mut found = false;
    // there might be multiple processes, so just be able to kill them all encase
    for process_info in system.processes_by_name(EXECUTABLE_NAME) {
        ccprintln(window, "Killing existing RLBot process".to_owned());
        process_info.kill();
        found = true;
    }

    if !found {
        ccprintln(window, "No existing RLBot process found".to_owned());
    }
}