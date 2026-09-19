use std::path::PathBuf;

use crate::error::{AppError, AppResult};
use crate::models::OperationEvent;

pub trait EventSink: Send + Sync {
    fn send(&self, event: OperationEvent);
}

impl<T: EventSink + ?Sized> EventSink for std::sync::Arc<T> {
    fn send(&self, event: OperationEvent) {
        self.as_ref().send(event);
    }
}

#[derive(Clone)]
pub struct RuntimeContext {
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl RuntimeContext {
    pub fn new(data_dir: PathBuf, cache_dir: PathBuf) -> Self {
        Self {
            data_dir,
            cache_dir,
        }
    }

    pub fn from_environment() -> AppResult<Self> {
        let data_dir = dirs_path("XDG_DATA_HOME")
            .or_else(|| home_path().map(|home| home.join(".local/share")))
            .ok_or_else(|| AppError::message("Could not determine the application data folder."))?
            .join("project-sunrise-launcher");
        let cache_dir = dirs_path("XDG_CACHE_HOME")
            .or_else(|| home_path().map(|home| home.join(".cache")))
            .ok_or_else(|| AppError::message("Could not determine the application cache folder."))?
            .join("project-sunrise-launcher");
        Ok(Self::new(data_dir, cache_dir))
    }
}

fn dirs_path(variable: &str) -> Option<PathBuf> {
    std::env::var_os(variable).map(PathBuf::from)
}

fn home_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}
