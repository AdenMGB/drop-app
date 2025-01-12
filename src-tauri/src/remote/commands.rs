use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};
use url::Url;

use crate::{
    error::{remote_access_error::RemoteAccessError, user_error::UserValue},
    AppState, AppStatus, DB,
};

use super::{
    auth::{auth_initiate_logic, recieve_handshake, setup},
    remote::use_remote_logic,
};

#[tauri::command]
pub fn use_remote(
    url: String,
    state: tauri::State<'_, Mutex<AppState<'_>>>,
) -> UserValue<(), RemoteAccessError> {
    UserValue::Ok(use_remote_logic(url, state)?)
}

#[tauri::command]
pub fn gen_drop_url(path: String) -> UserValue<String, RemoteAccessError> {
    let base_url = {
        let handle = DB.borrow_data().unwrap();

        Url::parse(&handle.base_url).map_err(RemoteAccessError::ParsingError)?
    };

    let url = base_url.join(&path).unwrap();

    UserValue::Ok(url.to_string())
}

#[tauri::command]
pub fn sign_out(app: AppHandle) {
    // Clear auth from database
    {
        let mut handle = DB.borrow_data_mut().unwrap();
        handle.auth = None;
        drop(handle);
        DB.save().unwrap();
    }

    // Update app state
    {
        let app_state = app.state::<Mutex<AppState>>();
        let mut app_state_handle = app_state.lock().unwrap();
        app_state_handle.status = AppStatus::SignedOut;
        app_state_handle.user = None;
    }

    // Emit event for frontend
    app.emit("auth/signedout", ()).unwrap();
}

// TODO: Fix / clarify the return values for this command
#[tauri::command]
pub fn retry_connect(state: tauri::State<'_, Mutex<AppState>>) {
    let (app_status, user) = setup();

    let mut guard = state.lock().unwrap();
    guard.status = app_status;
    guard.user = user;
    drop(guard);
}

#[tauri::command]
pub fn auth_initiate() -> UserValue<(), RemoteAccessError> {
    auth_initiate_logic().into()
}

#[tauri::command]
pub fn manual_recieve_handshake(app: AppHandle, token: String) {
    recieve_handshake(app, format!("handshake/{}", token));
}
