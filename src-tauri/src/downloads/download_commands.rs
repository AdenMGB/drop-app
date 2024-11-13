use std::sync::Mutex;

use crate::AppState;

#[tauri::command]
pub fn download_game(
    game_id: String,
    game_version: String,
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    /*
    info!("beginning game download...");

    let mut download_agent = GameDownloadAgent::new(game_id.clone(), game_version.clone(), 0);
    // Setup download requires mutable
    download_agent.setup_download().unwrap();

    let mut lock: std::sync::MutexGuard<'_, AppState> = state.lock().unwrap();
    let download_agent_ref = Arc::new(download_agent);
    lock.download_manager
        .insert(game_id, download_agent_ref.clone());

    // Run it in another thread
    spawn(move || {
        // Run doesn't require mutable
        download_agent_ref.clone().run();
    });
    */
    state
        .lock()
        .unwrap()
        .download_manager
        .queue_game(game_id, game_version, 0)
        .unwrap();
    Ok(())
}

#[tauri::command]
pub fn get_game_download_progress(
    state: tauri::State<'_, Mutex<AppState>>,
    game_id: String,
) -> Result<f64, String> {
    /*
    let download_agent = use_download_agent(state, game_id)?;

    let progress = &download_agent.progress;

    Ok(progress.get_progress())
    */
    let progress = state
        .lock()
        .unwrap()
        .download_manager
        .get_current_game_download_progress()
        .unwrap_or(0.0);

    Ok(progress)
}
/*
fn use_download_agent(
    state: tauri::State<'_, Mutex<AppState>>,
    game_id: String,
) -> Result<Arc<GameDownloadAgent>, String> {
    let lock = state.lock().unwrap();
    let download_agent = lock.download_manager.get(&game_id).ok_or("Invalid game ID")?;
    Ok(download_agent.clone()) // Clones the Arc, not the underlying data structure
}
*/
