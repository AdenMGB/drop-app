use std::{
    fmt::{Display, Formatter},
    sync::Mutex,
};

use log::{info, warn};
use serde::Deserialize;
use url::{ParseError, Url};

use crate::{AppState, AppStatus, DB};

#[derive(Debug)]
pub enum RemoteAccessError {
    FetchError(reqwest::Error),
    ParsingError(ParseError),
    InvalidCodeError(u16),
    GenericErrror(String),
}

impl Display for RemoteAccessError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            RemoteAccessError::FetchError(error) => write!(f, "{}", error),
            RemoteAccessError::GenericErrror(error) => write!(f, "{}", error),
            RemoteAccessError::ParsingError(parse_error) => {
                write!(f, "{}", parse_error)
            }
            RemoteAccessError::InvalidCodeError(error) => write!(f, "HTTP {}", error),
        }
    }
}

impl From<reqwest::Error> for RemoteAccessError {
    fn from(err: reqwest::Error) -> Self {
        RemoteAccessError::FetchError(err)
    }
}
impl From<String> for RemoteAccessError {
    fn from(err: String) -> Self {
        RemoteAccessError::GenericErrror(err)
    }
}
impl From<ParseError> for RemoteAccessError {
    fn from(err: ParseError) -> Self {
        RemoteAccessError::ParsingError(err)
    }
}
impl From<u16> for RemoteAccessError {
    fn from(err: u16) -> Self {
        RemoteAccessError::InvalidCodeError(err)
    }
}

impl std::error::Error for RemoteAccessError {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DropHealthcheck {
    app_name: String,
}

async fn use_remote_logic<'a>(
    url: String,
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<(), RemoteAccessError> {
    info!("connecting to url {}", url);
    let base_url = Url::parse(&url)?;

    // Test Drop url
    let test_endpoint = base_url.join("/api/v1")?;
    let response = reqwest::get(test_endpoint.to_string()).await?;

    let result = response.json::<DropHealthcheck>().await?;

    if result.app_name != "Drop" {
        warn!("user entered drop endpoint that connected, but wasn't identified as Drop");
        return Err("Not a valid Drop endpoint".to_string().into());
    }

    let mut app_state = state.lock().unwrap();
    app_state.status = AppStatus::SignedOut;
    drop(app_state);

    let mut db_state = DB.borrow_data_mut().unwrap();
    db_state.base_url = base_url.to_string();
    drop(db_state);

    DB.save().unwrap();

    Ok(())
}

#[tauri::command]
pub async fn use_remote<'a>(
    url: String,
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let result = use_remote_logic(url, state).await;

    if result.is_err() {
        return Err(result.err().unwrap().to_string());
    }

    Ok(())
}

#[tauri::command]
pub fn gen_drop_url(path: String) -> Result<String, String> {
    let base_url = {
        let handle = DB.borrow_data().unwrap();

        if handle.base_url.is_empty() {
            return Ok("".to_string());
        };

        Url::parse(&handle.base_url).unwrap()
    };

    let url = base_url.join(&path).unwrap();

    Ok(url.to_string())
}
