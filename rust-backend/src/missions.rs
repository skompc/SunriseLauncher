use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::github::GitHubClient;
use crate::installer::extract_zip;

const OWNER: &str = "stanuwu";
const REPOSITORY: &str = "SunriseMissions";
const BRANCH: &str = "main";

pub fn scripts_path(install_root: &Path) -> PathBuf {
    install_root
        .join("bin")
        .join("x64")
        .join("Sunrise")
        .join("scripts")
}

// A git checkout is managed by git; replacing it would drop its history and local work.
pub fn is_git_checkout(install_root: &Path) -> bool {
    scripts_path(install_root).join(".git").exists()
}

/**
 * Replaces the scripts folder with the latest commit of the missions repository.
 * @return The installed commit.
 */
pub async fn install_latest(github: &GitHubClient, install_root: &Path) -> AppResult<String> {
    let scripts = scripts_path(install_root);
    if is_git_checkout(install_root) {
        return Err(AppError::message(
            "The scripts folder is a git checkout. Update it with git instead.",
        ));
    }
    let commit = github.latest_commit(OWNER, REPOSITORY, BRANCH).await?;

    let sunrise = scripts.parent().expect("scripts has a parent");
    std::fs::create_dir_all(sunrise)
        .map_err(|error| AppError::io("Could not create the Sunrise folder", error))?;
    // Staged next to the target so the final swap is a rename on one volume.
    let staging = tempfile::Builder::new()
        .prefix(".missions-")
        .tempdir_in(sunrise)
        .map_err(|error| AppError::io("Could not create a temporary missions folder", error))?;
    let archive = staging.path().join("missions.zip");
    github
        .download_source(OWNER, REPOSITORY, &commit, &archive)
        .await?;
    let extracted = staging.path().join("extracted");
    let archive_copy = archive.clone();
    let extracted_copy = extracted.clone();
    tokio::task::spawn_blocking(move || extract_zip(&archive_copy, &extracted_copy))
        .await
        .map_err(|error| {
            AppError::message(format!("Archive extraction stopped unexpectedly: {error}"))
        })??;
    let source = archive_root(&extracted)?;
    remove_dot_entries(&source)?;

    let previous = staging.path().join("previous");
    if scripts.exists() {
        std::fs::rename(&scripts, &previous).map_err(|error| {
            AppError::io(
                "Could not replace the scripts folder; close Destiny 2 and any program using it",
                error,
            )
        })?;
    }
    if let Err(error) = std::fs::rename(&source, &scripts) {
        if previous.exists() {
            let _ = std::fs::rename(&previous, &scripts);
        }
        return Err(AppError::io("Could not install the mission scripts", error));
    }
    Ok(commit)
}

// GitHub source archives hold one top-level folder named after the repository and commit.
fn archive_root(extracted: &Path) -> AppResult<PathBuf> {
    let mut entries = std::fs::read_dir(extracted)
        .map_err(|error| AppError::io("Could not read the missions archive", error))?
        .filter_map(Result::ok);
    match (entries.next(), entries.next()) {
        (Some(entry), None) if entry.path().is_dir() => Ok(entry.path()),
        _ => Err(AppError::message(
            "The missions archive does not have the expected layout.",
        )),
    }
}

// Repository files such as .gitattributes are not scripts.
fn remove_dot_entries(root: &Path) -> AppResult<()> {
    let entries = std::fs::read_dir(root)
        .map_err(|error| AppError::io("Could not read the missions archive", error))?;
    for entry in entries.filter_map(Result::ok) {
        if !entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        let removed = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        removed.map_err(|error| AppError::io("Could not prepare the mission scripts", error))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{archive_root, remove_dot_entries};

    #[test]
    fn archive_root_needs_exactly_one_folder() {
        let temporary = tempfile::tempdir().expect("temporary folder");
        assert!(archive_root(temporary.path()).is_err());

        let root = temporary.path().join("SunriseMissions-abc");
        std::fs::create_dir_all(root.join("lib")).expect("archive folder");
        std::fs::write(root.join(".gitattributes"), b"* text").expect("dot file");
        std::fs::write(root.join("mission.lua"), b"return {}").expect("script");
        assert_eq!(archive_root(temporary.path()).expect("root"), root);

        remove_dot_entries(&root).expect("dot files removed");
        assert!(!root.join(".gitattributes").exists());
        assert!(root.join("mission.lua").exists());
        assert!(root.join("lib").is_dir());

        std::fs::write(temporary.path().join("stray.txt"), b"x").expect("stray file");
        assert!(archive_root(temporary.path()).is_err());
    }
}
