use crate::bot_management::{
    bot_creation::{bootstrap_python_bot, bootstrap_python_hivemind, bootstrap_rust_bot, bootstrap_scratch_bot, CREATED_BOTS_FOLDER},
    downloader,
};
use crate::rlbot::{
    agents::runnable::Runnable,
    gateway_util,
    parsing::bot_config_bundle::{BotConfigBundle, ScriptConfigBundle},
    setup_manager,
};
use crate::settings::*;
use crate::*;
use rayon::iter::{IndexedParallelIterator, IntoParallelRefIterator, ParallelIterator};
use std::{
    collections::HashMap,
    fs::{create_dir_all, File},
    io::{copy, Cursor, Write},
    path::Path,
    process::Command,
};
use tauri::Window;

#[tauri::command]
pub async fn check_rlbot_python() -> HashMap<String, bool> {
    let mut python_support = HashMap::new();

    let python_path = PYTHON_PATH.lock().unwrap().to_string();

    if get_command_status(&python_path, vec!["--version"]) {
        python_support.insert("python".to_string(), true);
        python_support.insert(
            "rlbotpython".to_string(),
            get_command_status(&python_path, vec!["-c", "import rlbot; import numpy; import numba; import scipy; import selenium"]),
        );
    } else {
        python_support.insert("python".to_string(), false);
        python_support.insert("rlbotpython".to_string(), false);
    }

    dbg!(python_support)
}

fn ensure_bot_directory() -> String {
    let bot_directory = get_content_folder();
    let bot_directory_path = Path::new(&bot_directory).join(CREATED_BOTS_FOLDER);

    if !bot_directory_path.exists() {
        create_dir_all(&bot_directory_path).unwrap();
    }

    bot_directory.to_string_lossy().to_string()
}

#[tauri::command]
pub async fn begin_python_bot(bot_name: String) -> Result<HashMap<String, BotConfigBundle>, HashMap<String, String>> {
    match bootstrap_python_bot(bot_name, &ensure_bot_directory()).await {
        Ok(config_file) => Ok(HashMap::from([("bot".to_string(), BotConfigBundle::minimal_from_path(Path::new(&config_file)).unwrap())])),
        Err(e) => Err(HashMap::from([("error".to_string(), e)])),
    }
}

#[tauri::command]
pub async fn begin_python_hivemind(hive_name: String) -> Result<HashMap<String, BotConfigBundle>, HashMap<String, String>> {
    match bootstrap_python_hivemind(hive_name, &ensure_bot_directory()).await {
        Ok(config_file) => Ok(HashMap::from([("bot".to_string(), BotConfigBundle::minimal_from_path(Path::new(&config_file)).unwrap())])),
        Err(e) => Err(HashMap::from([("error".to_string(), e)])),
    }
}

#[tauri::command]
pub async fn begin_rust_bot(bot_name: String) -> Result<HashMap<String, BotConfigBundle>, HashMap<String, String>> {
    match bootstrap_rust_bot(bot_name, &ensure_bot_directory()).await {
        Ok(config_file) => Ok(HashMap::from([("bot".to_string(), BotConfigBundle::minimal_from_path(Path::new(&config_file)).unwrap())])),
        Err(e) => Err(HashMap::from([("error".to_string(), e)])),
    }
}

#[tauri::command]
pub async fn begin_scratch_bot(bot_name: String) -> Result<HashMap<String, BotConfigBundle>, HashMap<String, String>> {
    match bootstrap_scratch_bot(bot_name, &ensure_bot_directory()).await {
        Ok(config_file) => Ok(HashMap::from([("bot".to_string(), BotConfigBundle::minimal_from_path(Path::new(&config_file)).unwrap())])),
        Err(e) => Err(HashMap::from([("error".to_string(), e)])),
    }
}

#[tauri::command]
pub async fn install_package(package_string: String) -> PackageResult {
    let exit_code = spawn_capture_process_and_get_exit_code(
        PYTHON_PATH.lock().unwrap().to_string(),
        &["-m", "pip", "install", "-U", "--no-warn-script-location", &package_string],
    );

    PackageResult {
        exit_code,
        packages: vec![package_string],
    }
}

#[tauri::command]
pub async fn install_requirements(config_path: String) -> PackageResult {
    let bundle = BotConfigBundle::minimal_from_path(Path::new(&config_path)).unwrap();

    if let Some(file) = bundle.get_requirements_file() {
        let packages = bundle.get_missing_packages();
        let python = PYTHON_PATH.lock().unwrap().to_string();
        let exit_code = spawn_capture_process_and_get_exit_code(&python, &["-m", "pip", "install", "-U", "--no-warn-script-location", "-r", file]);

        PackageResult { exit_code, packages }
    } else {
        PackageResult {
            exit_code: 1,
            packages: vec!["Unknown file".to_owned()],
        }
    }
}

const INSTALL_BASIC_PACKAGES_ARGS: [&[&str]; 4] = [
    &["-m", "ensurepip"],
    &["-m", "pip", "install", "-U", "--no-warn-script-location", "pip"],
    &["-m", "pip", "install", "-U", "--no-warn-script-location", "setuptools", "wheel"],
    &["-m", "pip", "install", "-U", "--no-warn-script-location", "numpy", "scipy", "numba", "selenium", "rlbot"],
];

fn install_upgrade_basic_packages() -> PackageResult {
    let packages = vec![
        String::from("pip"),
        String::from("setuptools"),
        String::from("wheel"),
        String::from("numpy"),
        String::from("scipy"),
        String::from("numba"),
        String::from("selenium"),
        String::from("rlbot"),
    ];

    let python = PYTHON_PATH.lock().unwrap().to_string();

    let mut exit_code = 0;

    for command in INSTALL_BASIC_PACKAGES_ARGS {
        if exit_code != 0 {
            break;
        }

        exit_code = spawn_capture_process_and_get_exit_code(&python, command);
    }

    PackageResult { exit_code, packages }
}

#[tauri::command]
pub async fn install_basic_packages() -> PackageResult {
    install_upgrade_basic_packages()
}

#[tauri::command]
pub async fn get_console_texts() -> Vec<ConsoleText> {
    CONSOLE_TEXT.lock().unwrap().clone()
}

#[tauri::command]
pub async fn get_missing_bot_packages(bots: Vec<BotConfigBundle>) -> Vec<MissingPackagesUpdate> {
    if check_has_rlbot() {
        bots.par_iter()
            .enumerate()
            .filter_map(|(index, bot)| {
                if bot.runnable_type == *"rlbot" {
                    let mut warn = bot.warn.clone();
                    let mut missing_packages = bot.missing_python_packages.clone();

                    if let Some(missing_packages) = &missing_packages {
                        if warn == Some("pythonpkg".to_string()) && missing_packages.is_empty() {
                            warn = None;
                        }
                    } else {
                        let bot_missing_packages = bot.get_missing_packages();

                        if !bot_missing_packages.is_empty() {
                            warn = Some("pythonpkg".to_string());
                        } else {
                            warn = None;
                        }

                        missing_packages = Some(bot_missing_packages);
                    }

                    if warn != bot.warn || missing_packages != bot.missing_python_packages {
                        return Some(MissingPackagesUpdate { index, warn, missing_packages });
                    }
                }

                None
            })
            .collect()
    } else {
        bots.par_iter()
            .enumerate()
            .filter_map(|(index, bot)| {
                if bot.runnable_type == *"rlbot" && (bot.warn.is_some() || bot.missing_python_packages.is_some()) {
                    Some(MissingPackagesUpdate {
                        index,
                        warn: None,
                        missing_packages: None,
                    })
                } else {
                    None
                }
            })
            .collect()
    }
}

#[tauri::command]
pub async fn get_missing_script_packages(scripts: Vec<ScriptConfigBundle>) -> Vec<MissingPackagesUpdate> {
    if check_has_rlbot() {
        scripts
            .par_iter()
            .enumerate()
            .filter_map(|(index, script)| {
                let mut warn = script.warn.clone();
                let mut missing_packages = script.missing_python_packages.clone();

                if let Some(missing_packages) = &missing_packages {
                    if warn == Some("pythonpkg".to_string()) && missing_packages.is_empty() {
                        warn = None;
                    }
                } else {
                    let script_missing_packages = script.get_missing_packages();

                    if !script_missing_packages.is_empty() {
                        warn = Some("pythonpkg".to_string());
                    } else {
                        warn = None;
                    }

                    missing_packages = Some(script_missing_packages);
                }

                if warn != script.warn || missing_packages != script.missing_python_packages {
                    Some(MissingPackagesUpdate { index, warn, missing_packages })
                } else {
                    None
                }
            })
            .collect()
    } else {
        scripts
            .par_iter()
            .enumerate()
            .filter_map(|(index, script)| {
                if script.warn.is_some() || script.missing_python_packages.is_some() {
                    Some(MissingPackagesUpdate {
                        index,
                        warn: None,
                        missing_packages: None,
                    })
                } else {
                    None
                }
            })
            .collect()
    }
}

#[tauri::command]
pub async fn get_missing_bot_logos(bots: Vec<BotConfigBundle>) -> Vec<LogoUpdate> {
    bots.par_iter()
        .enumerate()
        .filter_map(|(index, bot)| {
            if bot.runnable_type == *"rlbot" && bot.logo.is_none() {
                if let Some(logo) = bot.get_logo() {
                    return Some(LogoUpdate { index, logo });
                }
            }

            None
        })
        .collect()
}

#[tauri::command]
pub async fn get_missing_script_logos(scripts: Vec<ScriptConfigBundle>) -> Vec<LogoUpdate> {
    scripts
        .par_iter()
        .enumerate()
        .filter_map(|(index, script)| {
            if script.logo.is_none() {
                if let Some(logo) = script.get_logo() {
                    return Some(LogoUpdate { index, logo });
                }
            }

            None
        })
        .collect()
}

#[tauri::command]
pub fn is_windows() -> bool {
    cfg!(windows)
}

#[tauri::command]
pub async fn install_python() -> Option<u8> {
    // https://www.python.org/ftp/python/3.7.9/python-3.7.9-amd64.exe
    // download the above file to python-3.7.9-amd64.exe

    let file_path = get_content_folder().join("python-3.7.9-amd64.exe");

    if !file_path.exists() {
        let response = reqwest::get("https://www.python.org/ftp/python/3.7.9/python-3.7.9-amd64.exe").await.ok()?;
        let mut file = File::create(&file_path).ok()?;
        let mut content = Cursor::new(response.bytes().await.ok()?);
        copy(&mut content, &mut file).ok()?;
    }

    // only installs for the current user (requires no admin privileges)
    // adds the Python version to PATH
    // Launches the installer in a simplified mode for a one-button install
    let mut process = Command::new(file_path)
        .args([
            "InstallLauncherAllUsers=0",
            "SimpleInstall=1",
            "PrependPath=1",
            "SimpleInstallDescription='Install Python 3.7.9 for the current user to use with RLBot'",
        ])
        .spawn()
        .ok()?;
    process.wait().ok()?;

    // Windows actually doesn't have a python3.7.exe command, just python.exe (no matter what)
    // but there is a pip3.7.exe
    // Since we added Python to PATH, we can use where to find the path to pip3.7.exe
    // we can then use that to find the path to the right python.exe and use that
    let new_python_path = {
        let output = Command::new("where").arg("pip3.7").output().ok()?;
        let stdout = String::from_utf8(output.stdout).ok()?;
        Path::new(stdout.lines().next()?).parent().unwrap().parent().unwrap().join("python.exe")
    };
    *PYTHON_PATH.lock().unwrap() = new_python_path.to_string_lossy().to_string();

    Some(0)
}

#[tauri::command]
pub async fn download_bot_pack(window: Window) -> String {
    let botpack_location = get_content_folder().join(BOTPACK_FOLDER).to_string_lossy().to_string();
    let botpack_status = downloader::download_repo(&window, BOTPACK_REPO_OWNER, BOTPACK_REPO_NAME, &botpack_location, true).await;

    match botpack_status {
        downloader::BotpackStatus::Success(message) => {
            // Configure the folder settings
            BOT_FOLDER_SETTINGS.lock().unwrap().add_folder(botpack_location);
            message
        }
        downloader::BotpackStatus::Skipped(message) => message,
        _ => unreachable!(),
    }
}

#[tauri::command]
pub async fn update_bot_pack(window: Window) -> String {
    let botpack_location = get_content_folder().join(BOTPACK_FOLDER).to_string_lossy().to_string();
    let botpack_status = downloader::update_bot_pack(&window, BOTPACK_REPO_OWNER, BOTPACK_REPO_NAME, &botpack_location).await;

    match botpack_status {
        downloader::BotpackStatus::Skipped(message) => message,
        downloader::BotpackStatus::Success(message) => {
            // Configure the folder settings
            BOT_FOLDER_SETTINGS.lock().unwrap().add_folder(botpack_location);
            message
        }
        downloader::BotpackStatus::RequiresFullDownload => {
            // We need to download the botpack
            // the most likely cause is the botpack not existing in the first place
            match downloader::download_repo(&window, BOTPACK_REPO_OWNER, BOTPACK_REPO_NAME, &botpack_location, true).await {
                downloader::BotpackStatus::Success(message) => {
                    BOT_FOLDER_SETTINGS.lock().unwrap().add_folder(botpack_location);
                    message
                }
                downloader::BotpackStatus::Skipped(message) => message,
                _ => unreachable!(),
            }
        }
    }
}

#[tauri::command]
pub async fn update_map_pack(window: Window) -> String {
    let mappack_location = get_content_folder().join(MAPPACK_FOLDER);
    let updater = downloader::MapPackUpdater::new(&mappack_location, MAPPACK_REPO.0.to_string(), MAPPACK_REPO.1.to_string());
    let location = mappack_location.to_string_lossy().to_string();
    let map_index_old = updater.get_map_index();

    match updater.needs_update().await {
        downloader::BotpackStatus::Skipped(message) => {
            BOT_FOLDER_SETTINGS.lock().unwrap().add_folder(location);
            message
        }
        downloader::BotpackStatus::Success(message) => {
            // Configure the folder settings
            BOT_FOLDER_SETTINGS.lock().unwrap().add_folder(location);
            message
        }
        downloader::BotpackStatus::RequiresFullDownload => {
            // We need to download the botpack
            // the most likely cause is the botpack not existing in the first place
            match downloader::download_repo(&window, MAPPACK_REPO.0, MAPPACK_REPO.1, &location, false).await {
                downloader::BotpackStatus::Success(message) => {
                    BOT_FOLDER_SETTINGS.lock().unwrap().add_folder(location);

                    if updater.get_map_index().is_none() {
                        ccprintlne("Couldn't find revision number in map pack".to_string());
                        return "Couldn't find revision number in map pack".to_string();
                    }

                    updater.hydrate_map_pack(map_index_old).await;

                    message
                }
                downloader::BotpackStatus::Skipped(message) => message,
                _ => unreachable!(),
            }
        }
    }
}

#[tauri::command]
pub async fn is_botpack_up_to_date() -> bool {
    let repo_full_name = format!("{}/{}", BOTPACK_REPO_OWNER, BOTPACK_REPO_NAME);
    bot_management::downloader::is_botpack_up_to_date(&repo_full_name).await
}

#[tauri::command]
pub async fn get_launcher_settings() -> LauncherSettings {
    LauncherSettings::new()
}

#[tauri::command]
pub async fn save_launcher_settings(settings: LauncherSettings) {
    settings.write_to_file();
}

fn create_match_handler() -> Option<ChildStdin> {
    let program = PYTHON_PATH.lock().unwrap().clone();
    let script_path = get_content_folder().join("match_handler.py").to_string_lossy().to_string();

    match get_capture_command(program, &[&script_path]).stdin(Stdio::piped()).spawn() {
        Ok(mut child) => child.stdin.take(),
        Err(err) => {
            ccprintlne(format!("Failed to start match handler: {}", err));
            None
        }
    }
}

pub fn issue_match_handler_command(command_parts: &[String], create_handler: bool) {
    let mut match_handler_stdin = MATCH_HANDLER_STDIN.lock().unwrap();

    if match_handler_stdin.is_none() {
        if create_handler {
            *match_handler_stdin = create_match_handler();
        } else {
            ccprintln("Not issuing command to handler as it's down and I was told to not start it".to_string());
            return;
        }
    }

    let command = format!("{}\n", command_parts.join(" | "));
    let stdin = match_handler_stdin.as_mut().unwrap();

    println!("Issuing the following command to the match handler: {}", command);
    if stdin.write_all(command.as_bytes()).is_err() {
        *match_handler_stdin = None;
        ccprintlne("Match handler is no longer accepting commands. Restarting!".to_string());
        issue_match_handler_command(command_parts, false);
    }
}

#[tauri::command]
pub async fn start_match(window: Window, bot_list: Vec<TeamBotBundle>, match_settings: MatchSettings) -> bool {
    let port = gateway_util::find_existing_process().unwrap_or(gateway_util::IDEAL_RLBOT_PORT);

    match setup_manager::is_rocket_league_running(port) {
        Ok(is_running) => ccprintln(format!(
            "Rocket League is {}",
            if is_running { "already running with RLBot args!" } else { "not running yet..." }
        )),
        Err(err) => {
            ccprintlne(err);
            return false;
        }
    }

    let launcher_settings = LauncherSettings::new();

    let match_settings = match match_settings.setup_for_start_match(&BOT_FOLDER_SETTINGS.lock().unwrap().folders) {
        Some(match_settings) => match_settings,
        None => {
            window.emit("match-start-failed", ()).unwrap();
            return false;
        }
    };

    let args = [
        "start_match".to_string(),
        serde_json::to_string(&bot_list).unwrap().as_str().to_string(),
        serde_json::to_string(&match_settings).unwrap().as_str().to_string(),
        launcher_settings.preferred_launcher,
        launcher_settings.use_login_tricks.to_string(),
        launcher_settings.rocket_league_exe_path.unwrap_or_default(),
    ];

    issue_match_handler_command(&args, true);

    true
}

#[tauri::command]
pub async fn kill_bots() {
    issue_match_handler_command(&["shut_down".to_string()], false);
    // gateway_util::kill_existing_processes();
}
