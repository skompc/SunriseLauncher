use std::path::Path;

use fs2::available_space;

// Below this much free space, a failed file write counts as a full disk.
const LOW_SPACE_BYTES: u64 = 1024 * 1024 * 1024;

#[cfg(windows)]
const DISK_FULL_OS_ERRORS: [i32; 2] = [112, 39];
#[cfg(not(windows))]
const DISK_FULL_OS_ERRORS: [i32; 1] = [28];

#[cfg(windows)]
const FILE_LOCKED_OS_ERRORS: [i32; 2] = [32, 33];
#[cfg(not(windows))]
const FILE_LOCKED_OS_ERRORS: [i32; 0] = [];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DepotFailure {
    WrongPassword,
    SteamGuardExpired,
    SignInExpired,
    SavedSignInRejected,
    RateLimited,
    AccountRestricted(String),
    SignInFailed(String),
    SteamUnavailable,
    ConnectionFailed,
    NotOwned,
    ContentUnavailable,
    DiskFull,
    FileLocked,
    FileWriteFailed,
}

impl DepotFailure {
    pub fn message(&self) -> String {
        match self {
            Self::WrongPassword => {
                "Steam did not accept the username or password. Check both and try again.".into()
            }
            Self::SteamGuardExpired => {
                "The Steam Guard code expired. Start again and enter a new code.".into()
            }
            Self::SignInExpired => {
                "The Steam sign-in request expired or was declined. Start again and approve it in the Steam Mobile App.".into()
            }
            Self::SavedSignInRejected => {
                "Your saved Steam sign-in is no longer valid. Start again to sign in.".into()
            }
            Self::RateLimited => {
                "Steam blocked sign-in because of too many attempts. Wait a while, then try again.".into()
            }
            Self::AccountRestricted(result) => format!(
                "Steam refused to sign in to this account ({result}). Check the account on the Steam website."
            ),
            Self::SignInFailed(result) => {
                format!("Steam sign-in failed ({result}). Try again.")
            }
            Self::SteamUnavailable => "Steam is not available right now. Try again later.".into(),
            Self::ConnectionFailed => {
                "Could not reach Steam. Check your internet connection and try again.".into()
            }
            Self::NotOwned => {
                "This Steam account cannot download Destiny 2. Sign in with the account that owns it.".into()
            }
            Self::ContentUnavailable => {
                "Steam did not provide the Destiny 2 files. Try again later.".into()
            }
            Self::DiskFull => {
                "The install drive is full. Free up space and try again.".into()
            }
            Self::FileLocked => {
                "A game file is in use by another program. Close Destiny 2 and anything using the install folder, then try again.".into()
            }
            Self::FileWriteFailed => {
                "Could not write the game files. Check that the install folder is writable and not blocked by antivirus.".into()
            }
        }
    }

    /** A failed write on a nearly full drive is reported as a full disk. */
    pub fn refine(self, install_root: &Path) -> Self {
        if self == Self::FileWriteFailed
            && available_space(install_root).is_ok_and(|free| free < LOW_SPACE_BYTES)
        {
            Self::DiskFull
        } else {
            self
        }
    }
}

/**
 * Reads DepotDownloader output and keeps the first fatal failure it reports.
 * Every line it classifies ends the run, even when the exit code is 0.
 */
pub struct DepotFailureScanner {
    failure: Option<DepotFailure>,
    disk_full: Vec<String>,
    file_locked: Vec<String>,
}

impl Default for DepotFailureScanner {
    fn default() -> Self {
        Self {
            failure: None,
            disk_full: os_messages(&DISK_FULL_OS_ERRORS),
            file_locked: os_messages(&FILE_LOCKED_OS_ERRORS),
        }
    }
}

impl DepotFailureScanner {
    pub fn observe_line(&mut self, line: &str) {
        if self.failure.is_none() {
            self.failure = self.classify(line.trim());
        }
    }

    pub fn failure(&self) -> Option<DepotFailure> {
        self.failure.clone()
    }

    fn classify(&self, line: &str) -> Option<DepotFailure> {
        if let Some(detail) = line.strip_prefix("Failed to authenticate with Steam: ") {
            return Some(classify_authentication(detail));
        }
        if let Some(result) = line.strip_prefix("Unable to login to Steam3: ") {
            return Some(classify_result(result.trim()));
        }
        if line.starts_with("Access token was rejected") {
            return Some(DepotFailure::SavedSignInRejected);
        }
        if line.starts_with("Unable to get license list") {
            return Some(DepotFailure::SteamUnavailable);
        }
        if line.starts_with("Could not connect to Steam after")
            || line.starts_with("Failed to find any server with chunk")
        {
            return Some(DepotFailure::ConnectionFailed);
        }
        if line.ends_with("is not available from this account.")
            || line.starts_with("No valid depot key for")
        {
            return Some(DepotFailure::NotOwned);
        }
        if line.starts_with("Couldn't find any depots to download")
            || (line.starts_with("Depot ")
                && (line.contains(" not listed for app ")
                    || line.ends_with("missing public subsection or manifest section.")))
            || (line.starts_with("Encountered ")
                && line.contains(" for depot manifest ")
                && line.ends_with("Aborting."))
            || (line.starts_with("Encountered ")
                && line.contains(" for chunk ")
                && line.ends_with("Aborting."))
        {
            return Some(DepotFailure::ContentUnavailable);
        }
        if let Some(detail) = line
            .strip_prefix("Failed to allocate file ")
            .or_else(|| line.strip_prefix("Failed to resize file to expected size "))
            .or_else(|| line.strip_prefix("Download failed to due to an unhandled exception: "))
            .or_else(|| line.strip_prefix("Unhandled exception. "))
        {
            return self.classify_io(detail, line.starts_with("Failed to"));
        }
        if line.starts_with("Error: Unable to create install directories") {
            return Some(DepotFailure::FileWriteFailed);
        }
        None
    }

    fn classify_io(&self, detail: &str, is_write: bool) -> Option<DepotFailure> {
        let detail = detail.to_lowercase();
        if contains_any(&detail, &self.disk_full)
            || detail.contains("not enough space on the disk")
            || detail.contains("no space left on device")
        {
            Some(DepotFailure::DiskFull)
        } else if contains_any(&detail, &self.file_locked)
            || detail.contains("being used by another process")
        {
            Some(DepotFailure::FileLocked)
        } else if is_write
            || detail.contains("ioexception")
            || detail.contains("unauthorizedaccessexception")
        {
            Some(DepotFailure::FileWriteFailed)
        } else {
            None
        }
    }
}

// SteamKit formats an authentication failure as "<message> with result <EResult>.".
fn classify_authentication(detail: &str) -> DepotFailure {
    let Some((message, result)) = detail.rsplit_once(" with result ") else {
        return DepotFailure::SignInFailed(detail.trim().into());
    };
    let result = result.trim().trim_end_matches('.');
    match (message, result) {
        ("Failed to send steam guard code", "Expired") => DepotFailure::SteamGuardExpired,
        ("Failed to poll status", "Expired" | "FileNotFound") => DepotFailure::SignInExpired,
        _ => classify_result(result),
    }
}

fn classify_result(result: &str) -> DepotFailure {
    match result {
        "InvalidPassword" | "AccountNotFound" | "InvalidName" => DepotFailure::WrongPassword,
        "RateLimitExceeded" | "AccountLoginDeniedThrottle" | "LimitExceeded" => {
            DepotFailure::RateLimited
        }
        "AccountDisabled" | "AccountLockedDown" | "Suspended" | "Banned" | "Revoked" => {
            DepotFailure::AccountRestricted(result.into())
        }
        "ServiceUnavailable" | "Busy" | "Timeout" | "TryAnotherCM" => {
            DepotFailure::SteamUnavailable
        }
        "NoConnection" | "ConnectFailed" | "RemoteDisconnect" => DepotFailure::ConnectionFailed,
        "Expired" => DepotFailure::SignInExpired,
        other => DepotFailure::SignInFailed(other.into()),
    }
}

// DepotDownloader prints the OS text for an IO error, in the OS language.
fn os_messages(codes: &[i32]) -> Vec<String> {
    codes
        .iter()
        .filter_map(|code| {
            let text = std::io::Error::from_raw_os_error(*code).to_string();
            let text = text
                .split(" (os error")
                .next()
                .unwrap_or_default()
                .trim()
                .trim_end_matches('.')
                .to_lowercase();
            (!text.is_empty()).then_some(text)
        })
        .collect()
}

fn contains_any(text: &str, needles: &[String]) -> bool {
    needles.iter().any(|needle| text.contains(needle.as_str()))
}

#[cfg(test)]
mod tests {
    use super::{DepotFailure, DepotFailureScanner};

    fn scan(lines: &[&str]) -> Option<DepotFailure> {
        let mut scanner = DepotFailureScanner::default();
        for line in lines {
            scanner.observe_line(line);
        }
        scanner.failure()
    }

    #[test]
    fn wrong_password_is_named_before_the_generic_lines() {
        assert_eq!(
            scan(&[
                "Failed to authenticate with Steam: Authentication failed with result InvalidPassword.",
                "Unable to get steam3 credentials.",
                "Error: InitializeSteam failed",
            ]),
            Some(DepotFailure::WrongPassword)
        );
    }

    #[test]
    fn steam_guard_and_confirmation_expiry_are_separate() {
        assert_eq!(
            scan(&[
                "Failed to authenticate with Steam: Failed to send steam guard code with result Expired."
            ]),
            Some(DepotFailure::SteamGuardExpired)
        );
        assert_eq!(
            scan(&[
                "Failed to authenticate with Steam: Failed to poll status with result FileNotFound."
            ]),
            Some(DepotFailure::SignInExpired)
        );
    }

    #[test]
    fn rate_limit_and_saved_token_are_classified() {
        assert_eq!(
            scan(&[
                "Failed to authenticate with Steam: Authentication failed with result RateLimitExceeded."
            ]),
            Some(DepotFailure::RateLimited)
        );
        assert_eq!(
            scan(&["Access token was rejected (Expired)."]),
            Some(DepotFailure::SavedSignInRejected)
        );
        assert_eq!(
            scan(&["Unable to login to Steam3: ServiceUnavailable"]),
            Some(DepotFailure::SteamUnavailable)
        );
    }

    #[test]
    fn unknown_authentication_text_keeps_its_detail() {
        assert_eq!(
            scan(&["Failed to authenticate with Steam: There are no allowed confirmations"]),
            Some(DepotFailure::SignInFailed(
                "There are no allowed confirmations".into()
            ))
        );
    }

    #[test]
    fn ownership_and_content_failures_are_classified() {
        assert_eq!(
            scan(&["Depot 1085661 is not available from this account."]),
            Some(DepotFailure::NotOwned)
        );
        assert_eq!(
            scan(&["Encountered 404 for depot manifest 1085661 123. Aborting."]),
            Some(DepotFailure::ContentUnavailable)
        );
        assert_eq!(
            scan(&["Could not connect to Steam after 10 tries"]),
            Some(DepotFailure::ConnectionFailed)
        );
    }

    #[test]
    fn disk_and_lock_failures_are_classified() {
        assert_eq!(
            scan(&[
                "Failed to allocate file D:\\Game\\a.pkg: There is not enough space on the disk. : 'D:\\Game\\a.pkg'"
            ]),
            Some(DepotFailure::DiskFull)
        );
        assert_eq!(
            scan(&[
                "Download failed to due to an unhandled exception: No space left on device : '/game/a.pkg'"
            ]),
            Some(DepotFailure::DiskFull)
        );
        assert_eq!(
            scan(&[
                "Failed to allocate file D:\\Game\\a.pkg: The process cannot access the file 'D:\\Game\\a.pkg' because it is being used by another process."
            ]),
            Some(DepotFailure::FileLocked)
        );
        assert_eq!(
            scan(&["Failed to allocate file D:\\Game\\a.pkg: Access to the path is denied."]),
            Some(DepotFailure::FileWriteFailed)
        );
    }

    #[test]
    fn retried_and_informational_lines_are_not_failures() {
        assert_eq!(
            scan(&[
                "Connection to Steam failed. Trying again (#1)...",
                "Encountered error downloading chunk abc: ServiceUnavailable",
                "Connection timeout downloading depot manifest 1 2. Retrying.",
                "The previous 2-factor auth code you have provided is incorrect.",
                "This account is protected by Steam Guard.",
                "Download failed to due to an unhandled exception: Object reference not set",
            ]),
            None
        );
    }
}
