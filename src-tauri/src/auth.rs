use std::{
    env,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use log::{info, warn};
use openssl::{ec::EcKey, hash::MessageDigest, pkey::PKey, sign::Signer};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use url::Url;

use crate::{
    db::{DatabaseAuth, DatabaseImpls},
    remote::RemoteAccessError,
    AppState, AppStatus, User, DB,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InitiateRequestBody {
    name: String,
    platform: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HandshakeRequestBody {
    client_id: String,
    token: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HandshakeResponse {
    private: String,
    certificate: String,
    id: String,
}

pub fn sign_nonce(private_key: String, nonce: String) -> Result<String, ()> {
    let client_private_key = EcKey::private_key_from_pem(private_key.as_bytes()).unwrap();
    let pkey_private_key = PKey::from_ec_key(client_private_key).unwrap();

    let mut signer = Signer::new(MessageDigest::sha256(), &pkey_private_key).unwrap();
    signer.update(nonce.as_bytes()).unwrap();
    let signature = signer.sign_to_vec().unwrap();

    let hex_signature = hex::encode(signature);

    Ok(hex_signature)
}

pub fn generate_authorization_header() -> String {
    let certs = {
        let db = DB.borrow_data().unwrap();
        db.auth.clone().unwrap()
    };

    let start = SystemTime::now();
    let timestamp = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    let nonce = timestamp.as_millis().to_string();

    let signature = sign_nonce(certs.private, nonce.clone()).unwrap();

    format!("Nonce {} {} {}", certs.client_id, nonce, signature)
}

pub fn fetch_user() -> Result<User, RemoteAccessError> {
    let base_url = DB.fetch_base_url();

    let endpoint = base_url.join("/api/v1/client/user")?;
    let header = generate_authorization_header();

    let client = reqwest::blocking::Client::new();
    let response = client
        .get(endpoint.to_string())
        .header("Authorization", header)
        .send()?;

    if response.status() != 200 {
        return Err(response.status().as_u16().into());
    }

    let user = response.json::<User>()?;

    Ok(user)
}

fn recieve_handshake_logic(app: &AppHandle, path: String) -> Result<(), RemoteAccessError> {
    let path_chunks: Vec<&str> = path.split("/").collect();
    if path_chunks.len() != 3 {
        app.emit("auth/failed", ()).unwrap();
        return Err(RemoteAccessError::GenericErrror(
            "Invalid number of handshake chunks".to_string(),
        ));
    }

    let base_url = {
        let handle = DB.borrow_data().unwrap();
        Url::parse(handle.base_url.as_str())?
    };

    let client_id = path_chunks.get(1).unwrap();
    let token = path_chunks.get(2).unwrap();
    let body = HandshakeRequestBody {
        client_id: client_id.to_string(),
        token: token.to_string(),
    };

    let endpoint = base_url.join("/api/v1/client/auth/handshake")?;
    let client = reqwest::blocking::Client::new();
    let response = client.post(endpoint).json(&body).send()?;
    let response_struct = response.json::<HandshakeResponse>()?;

    {
        let mut handle = DB.borrow_data_mut().unwrap();
        handle.auth = Some(DatabaseAuth {
            private: response_struct.private,
            cert: response_struct.certificate,
            client_id: response_struct.id,
        });
        drop(handle);
        DB.save().unwrap();
    }

    {
        let app_state = app.state::<Mutex<AppState>>();
        let mut app_state_handle = app_state.lock().unwrap();
        app_state_handle.status = AppStatus::SignedIn;
        app_state_handle.user = Some(fetch_user()?);
    }

    Ok(())
}

pub fn recieve_handshake(app: AppHandle, path: String) {
    // Tell the app we're processing
    app.emit("auth/processing", ()).unwrap();

    let handshake_result = recieve_handshake_logic(&app, path);
    if handshake_result.is_err() {
        app.emit("auth/failed", ()).unwrap();
        return;
    }

    app.emit("auth/finished", ()).unwrap();
}

async fn auth_initiate_wrapper() -> Result<(), RemoteAccessError> {
    let base_url = {
        let db_lock = DB.borrow_data().unwrap();
        Url::parse(&db_lock.base_url.clone())?
    };

    let endpoint = base_url.join("/api/v1/client/auth/initiate")?;
    let body = InitiateRequestBody {
        name: "Drop Desktop Client".to_string(),
        platform: env::consts::OS.to_string(),
    };

    let client = reqwest::Client::new();
    let response = client.post(endpoint.to_string()).json(&body).send().await?;

    if response.status() != 200 {
        return Err("Failed to create redirect URL. Please try again later."
            .to_string()
            .into());
    }

    let redir_url = response.text().await?;
    let complete_redir_url = base_url.join(&redir_url)?;

    info!("opening web browser to continue authentication");
    webbrowser::open(complete_redir_url.as_ref()).unwrap();

    Ok(())
}

#[tauri::command]
pub async fn auth_initiate<'a>() -> Result<(), String> {
    let result = auth_initiate_wrapper().await;
    if result.is_err() {
        return Err(result.err().unwrap().to_string());
    }

    Ok(())
}

pub fn setup() -> Result<(AppStatus, Option<User>), ()> {
    let data = DB.borrow_data().unwrap();

    // If we have certs, exit for now
    if data.auth.is_some() {
        let user_result = fetch_user();
        if user_result.is_err() {
            let error = user_result.err().unwrap();
            warn!("auth setup failed with: {}", error);
            match error {
                RemoteAccessError::FetchError(_) => {
                    return Ok((AppStatus::ServerUnavailable, None))
                }
                _ => return Ok((AppStatus::SignedInNeedsReauth, None)),
            }
        }
        return Ok((AppStatus::SignedIn, Some(user_result.unwrap())));
    }

    drop(data);

    Ok((AppStatus::SignedOut, None))
}
