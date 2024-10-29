use std::sync::{atomic::Ordering, Arc, Mutex};

use log::info;

use crate::{downloads::download_agent::GameDownloadAgent, AppState};

use super::download_agent::{GameDownloadError, GameDownloadState};

#[tauri::command]
pub async fn queue_game_download(
    game_id: String,
    game_version: String,
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<(), GameDownloadError> {
    info!("Queuing Game Download");
    let download_agent = Arc::new(GameDownloadAgent::new(game_id.clone(), game_version.clone()));
    download_agent.queue().await?;

    let mut queue = state.lock().unwrap();
    queue.game_downloads.insert(game_id, download_agent);
    Ok(())
}

#[tauri::command]
pub async fn start_game_downloads(
    max_threads: usize,
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<(), GameDownloadError> {
    info!("Downloading Games");
    loop {
        let mut current_id = String::new();
        let mut download_agent = None;
        {
            let lock = state.lock().unwrap();
            for (id, agent) in &lock.game_downloads {
                if agent.get_state() == GameDownloadState::Queued {
                    download_agent = Some(agent.clone());
                    current_id = id.clone();
                    info!("Got queued game to download");
                    break;
                }
            }
            if download_agent.is_none() {
                info!("No more games left to download");
                return Ok(())
            }
        };
        info!("Downloading game");
        {
            start_game_download(max_threads, download_agent.unwrap()).await?;
            let mut lock = state.lock().unwrap();
            lock.game_downloads.remove_entry(&current_id);
        }
    }
}

pub async fn start_game_download(
    max_threads: usize,
    download_agent: Arc<GameDownloadAgent>
) -> Result<(), GameDownloadError> {
    info!("Triggered Game Download");


    download_agent.ensure_manifest_exists().await?;

    let local_manifest = {
        let manifest = download_agent.manifest.lock().unwrap();
        (*manifest).clone().unwrap()
    };

    download_agent.generate_job_contexts(&local_manifest, download_agent.version.clone(), download_agent.id.clone()).unwrap();

    download_agent.begin_download(max_threads).await?;

    Ok(())
}

#[tauri::command]
pub async fn stop_specific_game_download(state: tauri::State<'_, Mutex<AppState>>, game_id: String) -> Result<(), String> {
    let lock = state.lock().unwrap();
    let download_agent = lock.game_downloads.get(&game_id).unwrap();

    let callback = download_agent.callback.clone();
    drop(lock);

    callback.store(true, Ordering::Release);

    return Ok(())
}