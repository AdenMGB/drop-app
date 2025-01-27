use std::sync::Mutex;

use crate::AppState;

#[tauri::command]
pub fn launch_game(game_id: String, state: tauri::State<'_, Mutex<AppState>>) -> Result<(), String> {
    let state_lock = state.lock().unwrap();
    let mut process_manager_lock = state_lock.process_manager.lock().unwrap();

    process_manager_lock.launch_game(game_id)?;

    drop(process_manager_lock);
    drop(state_lock);

    Ok(())
}
