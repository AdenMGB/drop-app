use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tauri::{AppHandle, Manager};
use urlencoding::encode;

use crate::db::{ApplicationTransientStatus, DatabaseImpls, GameDownloadStatus};
use crate::db::GameVersion;
use crate::download_manager::download_manager::DownloadStatus;
use crate::download_manager::downloadable_metadata::DownloadableMetadata;
use crate::process::process_manager::Platform;
use crate::remote::RemoteAccessError;
use crate::state::{GameStatusManager, GameStatusWithTransient};
use crate::{auth::generate_authorization_header, AppState, DB};

#[derive(serde::Serialize)]
pub struct FetchGameStruct {
    game: Game,
    status: GameStatusWithTransient,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    id: String,
    m_name: String,
    m_short_description: String,
    m_description: String,
    // mDevelopers
    // mPublishers
    m_icon_id: String,
    m_banner_id: String,
    m_cover_id: String,
    m_image_library: Vec<String>,
}
#[derive(serde::Serialize, Clone)]
pub struct GameUpdateEvent {
    pub game_id: String,
    pub status: (Option<GameDownloadStatus>, Option<ApplicationTransientStatus>),
}

#[derive(Serialize, Clone)]
pub struct QueueUpdateEventQueueData {
    pub meta: DownloadableMetadata,
    pub status: DownloadStatus,
    pub progress: f64,
}

#[derive(serde::Serialize, Clone)]
pub struct QueueUpdateEvent {
    pub queue: Vec<QueueUpdateEventQueueData>,
}

#[derive(serde::Serialize, Clone)]
pub struct StatsUpdateEvent {
    pub speed: usize,
    pub time: usize,
}

// Game version with some fields missing and size information
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameVersionOption {
    version_index: usize,
    version_name: String,
    platform: Platform,
    setup_command: String,
    launch_command: String,
    delta: bool,
    umu_id_override: Option<String>,
    // total_size: usize,
}

fn fetch_library_logic(app: AppHandle) -> Result<Vec<Game>, RemoteAccessError> {
    let base_url = DB.fetch_base_url();
    let library_url = base_url.join("/api/v1/client/user/library")?;

    let header = generate_authorization_header();

    let client = reqwest::blocking::Client::new();
    let response = client
        .get(library_url.to_string())
        .header("Authorization", header)
        .send()?;

    if response.status() != 200 {
        return Err(response.status().as_u16().into());
    }

    let games: Vec<Game> = response.json::<Vec<Game>>()?;

    let state = app.state::<Mutex<AppState>>();
    let mut handle = state.lock().unwrap();

    let mut db_handle = DB.borrow_data_mut().unwrap();

    for game in games.iter() {
        handle.games.insert(game.id.clone(), game.clone());
        if !db_handle.applications.game_statuses.contains_key(&game.id) {
            db_handle
                .applications
                .game_statuses
                .insert(game.id.clone(), GameDownloadStatus::Remote {});
        }
    }

    drop(handle);

    Ok(games)
}

#[tauri::command]
pub fn fetch_library(app: AppHandle) -> Result<Vec<Game>, String> {
    fetch_library_logic(app).map_err(|e| e.to_string())
}

fn fetch_game_logic(
    id: String,
    app: tauri::AppHandle,
) -> Result<FetchGameStruct, RemoteAccessError> {
    let state = app.state::<Mutex<AppState>>();
    let mut state_handle = state.lock().unwrap();

    let game = state_handle.games.get(&id);
    if let Some(game) = game {
        let status = GameStatusManager::fetch_state(&id);

        let data = FetchGameStruct {
            game: game.clone(),
            status,
        };

        return Ok(data);
    }

    let base_url = DB.fetch_base_url();

    let endpoint = base_url.join(&format!("/api/v1/game/{}", id))?;
    let header = generate_authorization_header();

    let client = reqwest::blocking::Client::new();
    let response = client
        .get(endpoint.to_string())
        .header("Authorization", header)
        .send()?;

    if response.status() == 404 {
        return Err(RemoteAccessError::GameNotFound);
    }
    if response.status() != 200 {
        return Err(RemoteAccessError::InvalidCodeError(
            response.status().into(),
        ));
    }

    let game = response.json::<Game>()?;
    state_handle.games.insert(id.clone(), game.clone());

    let mut db_handle = DB.borrow_data_mut().unwrap();

    db_handle
        .applications
        .game_statuses
        .entry(id.clone())
        .or_insert(GameDownloadStatus::Remote {});
    drop(db_handle);

    let status = GameStatusManager::fetch_state(&id);

    let data = FetchGameStruct {
        game: game.clone(),
        status,
    };

    Ok(data)
}

#[tauri::command]
pub fn fetch_game(game_id: String, app: tauri::AppHandle) -> Result<FetchGameStruct, String> {
    let result = fetch_game_logic(game_id, app);

    if result.is_err() {
        return Err(result.err().unwrap().to_string());
    }

    Ok(result.unwrap())
}

#[tauri::command]
pub fn fetch_game_status(id: String) -> Result<GameStatusWithTransient, String> {
    let status = GameStatusManager::fetch_state(&id);

    Ok(status)
}

fn fetch_game_verion_options_logic<'a>(
    game_id: String,
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<Vec<GameVersionOption>, RemoteAccessError> {
    let base_url = DB.fetch_base_url();

    let endpoint =
        base_url.join(format!("/api/v1/client/metadata/versions?id={}", game_id).as_str())?;
    let header = generate_authorization_header();

    let client = reqwest::blocking::Client::new();
    let response = client
        .get(endpoint.to_string())
        .header("Authorization", header)
        .send()?;

    if response.status() != 200 {
        return Err(RemoteAccessError::InvalidCodeError(
            response.status().into(),
        ));
    }

    let data = response.json::<Vec<GameVersionOption>>()?;

    let state_lock = state.lock().unwrap();
    let process_manager_lock = state_lock.process_manager.lock().unwrap();
    let data = data
        .into_iter()
        .filter(|v| process_manager_lock.valid_platform(&v.platform).unwrap())
        .collect::<Vec<GameVersionOption>>();
    drop(process_manager_lock);
    drop(state_lock);

    Ok(data)
}

#[tauri::command]
pub fn uninstall_game(
    game_id: String,
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let state_lock = state.lock().unwrap();
    let meta = get_current_meta(&game_id)?;

    state_lock.download_manager.uninstall_application(meta);
    drop(state_lock);

    Ok(())
}

pub fn get_current_meta(game_id: &String) -> Result<DownloadableMetadata, String> {
    match DB.borrow_data().unwrap().applications.installed_game_version.get(game_id) {
        Some(meta) => Ok(meta.clone()),
        None => Err(String::from("Could not find installed version")),
    }
}

#[tauri::command]
pub fn fetch_game_verion_options<'a>(
    game_id: String,
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<Vec<GameVersionOption>, String> {
    fetch_game_verion_options_logic(game_id, state).map_err(|e| e.to_string())
}

pub fn on_game_complete(
    meta: &DownloadableMetadata,
    install_dir: String,
    app_handle: &AppHandle,
) -> Result<(), RemoteAccessError> {
    // Fetch game version information from remote
    let base_url = DB.fetch_base_url();
    if meta.version.is_none() { return Err(RemoteAccessError::GameNotFound) }

    let endpoint = base_url.join(
        format!(
            "/api/v1/client/metadata/version?id={}&version={}",
            meta.id,
            encode(meta.version.as_ref().unwrap())
        )
        .as_str(),
    )?;
    let header = generate_authorization_header();

    let client = reqwest::blocking::Client::new();
    let response = client
        .get(endpoint.to_string())
        .header("Authorization", header)
        .send()?;

    let data = response.json::<GameVersion>()?;

    let mut handle = DB.borrow_data_mut().unwrap();
    handle
        .applications
        .game_versions
        .entry(meta.id.clone())
        .or_default()
        .insert(meta.version.clone().unwrap(), data.clone());
    handle
        .applications
        .installed_game_version
        .insert(meta.id.clone(), meta.clone());

    drop(handle);
    DB.save().unwrap();

    let status = if data.setup_command.is_empty() {
        GameDownloadStatus::Installed {
            version_name: meta.version.clone().unwrap(),
            install_dir,
        }
    } else {
        GameDownloadStatus::SetupRequired {
            version_name: meta.version.clone().unwrap(),
            install_dir,
        }
    };

    let mut db_handle = DB.borrow_data_mut().unwrap();
    db_handle
        .applications
        .game_statuses
        .insert(meta.id.clone(), status.clone());
    drop(db_handle);
    DB.save().unwrap();
    app_handle
        .emit(
            &format!("update_game/{}", meta.id),
            GameUpdateEvent {
                game_id: meta.id.clone(),
                status: (Some(status), None),
            },
        )
        .unwrap();

    Ok(())
}

pub fn push_game_update(app_handle: &AppHandle, meta: &DownloadableMetadata, status: GameStatusWithTransient) {
    app_handle
        .emit(
            &format!("update_game/{}", meta.id),
            GameUpdateEvent {
                game_id: meta.id.clone(),
                status,
            },
        )
        .unwrap();
}