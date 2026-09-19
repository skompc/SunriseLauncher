use std::sync::Arc;

use project_sunrise_launcher_lib::error::AppError;
use project_sunrise_launcher_lib::github::GitHubClient;
use project_sunrise_launcher_lib::installer;
use project_sunrise_launcher_lib::models::{
    AppSnapshot, OperationEvent, OperationRequest, Preferences, current_platform, resolve_language,
};
use project_sunrise_launcher_lib::runtime::{EventSink, RuntimeContext};
use project_sunrise_launcher_lib::storage;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::ChildStdin;
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Deserialize)]
struct Request {
    id: u64,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Serialize)]
struct Response<'a, T: Serialize> {
    id: u64,
    result: Option<&'a T>,
    error: Option<String>,
}

struct StdoutSink {
    output: Arc<Mutex<tokio::io::Stdout>>,
    request_id: u64,
}

impl EventSink for StdoutSink {
    fn send(&self, event: OperationEvent) {
        let output = self.output.clone();
        let request_id = self.request_id;
        tokio::spawn(async move {
            let line = json!({ "id": request_id, "event": event }).to_string();
            let mut output = output.lock().await;
            let _ = output.write_all(format!("{line}\n").as_bytes()).await;
            let _ = output.flush().await;
        });
    }
}

#[derive(Default)]
struct OperationState {
    cancel: RwLock<Option<CancellationToken>>,
    terminal_input: Arc<Mutex<Option<ChildStdin>>>,
}

#[tokio::main]
async fn main() -> Result<(), AppError> {
    let context = RuntimeContext::from_environment()?;
    let output = Arc::new(Mutex::new(tokio::io::stdout()));
    let state = Arc::new(OperationState::default());
    let mut input = BufReader::new(tokio::io::stdin()).lines();

    while let Some(line) = input
        .next_line()
        .await
        .map_err(|error| AppError::io("Could not read Electron request", error))?
    {
        let request: Request = match serde_json::from_str(&line) {
            Ok(request) => request,
            Err(error) => {
                write_error(&output, 0, format!("Invalid request: {error}")).await;
                continue;
            }
        };
        let context = context.clone();
        let output = output.clone();
        let state = state.clone();
        match request.method.as_str() {
            "get_app_snapshot" => {
                let result = get_app_snapshot(&context).await;
                write_result(&output, request.id, result).await;
            }
            "inspect_installation" => {
                let directory = request.params["installDirectory"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                write_result(
                    &output,
                    request.id,
                    storage::inspect_installation(&directory).await,
                )
                .await;
            }
            "save_preferences" => {
                let preferences: Preferences =
                    match serde_json::from_value(request.params["preferences"].clone()) {
                        Ok(value) => value,
                        Err(error) => {
                            write_error(&output, request.id, error.to_string()).await;
                            continue;
                        }
                    };
                write_result(
                    &output,
                    request.id,
                    storage::save_preferences(&context, &preferences).await,
                )
                .await;
            }
            "run_operation" => {
                let request_data = request.params["request"].clone();
                let operation: OperationRequest = match serde_json::from_value(request_data) {
                    Ok(value) => value,
                    Err(error) => {
                        write_error(&output, request.id, error.to_string()).await;
                        continue;
                    }
                };
                let cancel = CancellationToken::new();
                *state.cancel.write().await = Some(cancel.clone());
                let sink: Arc<dyn EventSink> = Arc::new(StdoutSink {
                    output: output.clone(),
                    request_id: request.id,
                });
                let state_for_task = state.clone();
                tokio::spawn(async move {
                    let result = storage::save_preferences(
                        &context,
                        &Preferences {
                            install_directory: operation.install_directory.clone(),
                            steam_username: operation.steam_username.clone(),
                            launch_command: operation.launch_command.clone(),
                            steam_language: resolve_language(&operation.steam_language)
                                .steam_language
                                .into(),
                            auth_method: operation.auth_method,
                        },
                    )
                    .await;
                    let result = match result {
                        Ok(()) => {
                            installer::run(
                                &context,
                                operation,
                                sink,
                                cancel,
                                state_for_task.terminal_input.clone(),
                            )
                            .await
                        }
                        Err(error) => Err(error),
                    };
                    write_result(&output, request.id, result).await;
                    *state_for_task.cancel.write().await = None;
                    *state_for_task.terminal_input.lock().await = None;
                });
            }
            "send_terminal_input" => {
                let input = request.params["input"].as_str().unwrap_or_default();
                let result = send_terminal_input(&state, input).await;
                write_result(&output, request.id, result).await;
            }
            "cancel_operation" => {
                let cancelled = state
                    .cancel
                    .read()
                    .await
                    .as_ref()
                    .map(CancellationToken::cancel)
                    .is_some();
                write_result(&output, request.id, Ok(cancelled)).await;
            }
            "launch_game" => {
                let install_directory = request.params["installDirectory"]
                    .as_str()
                    .unwrap_or_default();
                let result = launch_game(
                    install_directory,
                    request.params["launchCommand"].as_str().unwrap_or_default(),
                )
                .await;
                write_result(&output, request.id, result).await;
            }
            _ => write_error(&output, request.id, "Unknown backend method.".into()).await,
        }
    }
    Ok(())
}

async fn get_app_snapshot(context: &RuntimeContext) -> Result<AppSnapshot, AppError> {
    let mut preferences = storage::load_preferences(context).await?;
    preferences.steam_language = resolve_language(&preferences.steam_language)
        .steam_language
        .into();
    let installation = storage::inspect_installation(&preferences.install_directory).await?;
    let release_result = GitHubClient::new()?
        .latest_release("stanuwu", "Sunrise", "steam_api64.dll")
        .await;
    let (latest_release, release_error) = match release_result {
        Ok(release) => (Some(release), None),
        Err(error) => (None, Some(error.to_string())),
    };
    let update_available = latest_release
        .as_ref()
        .is_some_and(|release| installation.update_available(release));
    Ok(AppSnapshot {
        platform: current_platform(),
        preferences,
        installation,
        latest_release,
        update_available,
        release_error,
    })
}

async fn send_terminal_input(state: &OperationState, input: &str) -> Result<bool, AppError> {
    if input.len() > 512
        || input
            .chars()
            .any(|character| matches!(character, '\r' | '\n'))
    {
        return Err(AppError::message("The console response is not valid."));
    }
    let mut guard = state.terminal_input.lock().await;
    let Some(stdin) = guard.as_mut() else {
        return Ok(false);
    };
    stdin
        .write_all(format!("{input}\n").as_bytes())
        .await
        .map_err(|error| AppError::io("Could not send input to DepotDownloader", error))?;
    stdin
        .flush()
        .await
        .map_err(|error| AppError::io("Could not send input to DepotDownloader", error))?;
    Ok(true)
}

async fn launch_game(install_directory: &str, launch_command: &str) -> Result<bool, AppError> {
    let arguments = parse_command_line(launch_command)?;
    let (program, arguments) = arguments.split_first().ok_or_else(|| {
        AppError::message("Set a launch command in Settings before clicking Play.")
    })?;
    let directory = std::path::Path::new(install_directory.trim());
    if !directory.is_dir() {
        return Err(AppError::message("The installation folder does not exist."));
    }
    let mut command = std::process::Command::new(program);
    command.args(arguments).current_dir(directory);
    command
        .spawn()
        .map_err(|error| AppError::message(format!("Could not launch the game: {error}")))?;
    Ok(true)
}

fn parse_command_line(command: &str) -> Result<Vec<String>, AppError> {
    let mut arguments = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for character in command.trim().chars() {
        if matches!(character, '\'' | '"') {
            if quote == Some(character) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(character);
            } else {
                current.push(character);
            }
        } else if character.is_whitespace() && quote.is_none() {
            if !current.is_empty() {
                arguments.push(std::mem::take(&mut current));
            }
        } else {
            current.push(character);
        }
    }
    if quote.is_some() {
        return Err(AppError::message(
            "The launch command has an unmatched quote.",
        ));
    }
    if !current.is_empty() {
        arguments.push(current);
    }
    Ok(arguments)
}

async fn write_result<T: Serialize>(
    output: &Arc<Mutex<tokio::io::Stdout>>,
    id: u64,
    result: Result<T, AppError>,
) {
    match result {
        Ok(result) => {
            write_json(
                output,
                json!(Response {
                    id,
                    result: Some(&result),
                    error: None
                }),
            )
            .await
        }
        Err(error) => write_error(output, id, error.to_string()).await,
    }
}

async fn write_error(output: &Arc<Mutex<tokio::io::Stdout>>, id: u64, error: String) {
    write_json(
        output,
        json!(Response::<Value> {
            id,
            result: None,
            error: Some(error)
        }),
    )
    .await;
}

async fn write_json(output: &Arc<Mutex<tokio::io::Stdout>>, value: Value) {
    let mut output = output.lock().await;
    let _ = output.write_all(format!("{value}\n").as_bytes()).await;
    let _ = output.flush().await;
}
