//! Talking to Music.app through `osascript`.
//!
//! Every call is blocking and takes tens of milliseconds, so callers run
//! them on the runtime's blocking pool, never on the UI thread. Scripts are
//! compiled into the binary and fed to `osascript` on standard input;
//! arguments travel as process arguments, never spliced into source.

use std::path::Path;

use serde::Deserialize;

const SNAPSHOT: &str = include_str!("snapshot.js");
const CONTROL: &str = include_str!("control.js");
const STATE: &str = include_str!("state.js");
const ARTWORK: &str = include_str!("artwork.applescript");

/// The name of the playlist Spotifast plays albums, artists and song lists
/// from. Kept in step with `QUEUE_NAME` in control.js.
pub const QUEUE_PLAYLIST: &str = "Spotifast Queue";

#[derive(Clone, Copy)]
enum Language {
    JavaScript,
    AppleScript,
}

#[cfg(target_os = "macos")]
fn osascript(language: Language, source: &str, args: &[&str]) -> Result<String, String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut command = Command::new("/usr/bin/osascript");
    if let Language::JavaScript = language {
        command.args(["-l", "JavaScript"]);
    }
    let mut child = command
        .arg("-")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("couldn't start osascript: {error}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "osascript has no input".to_string())?
        .write_all(source.as_bytes())
        .map_err(|error| format!("couldn't send the script to osascript: {error}"))?;
    let output = child
        .wait_with_output()
        .map_err(|error| format!("osascript failed: {error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(describe_failure(&String::from_utf8_lossy(&output.stderr)))
    }
}

#[cfg(not(target_os = "macos"))]
fn osascript(_language: Language, _source: &str, _args: &[&str]) -> Result<String, String> {
    Err("Apple Music needs Music.app on macOS".into())
}

/// osascript's error text, with the automation refusal macOS gives when the
/// user has not allowed control of Music.app said in plain words.
fn describe_failure(stderr: &str) -> String {
    let stderr = stderr.trim();
    if stderr.contains("-1743") || stderr.contains("Not authorized to send Apple events") {
        return "Spotifast isn't allowed to control Music. Allow it in System Settings → \
                Privacy & Security → Automation."
            .into();
    }
    // "execution error: Error: track not found: X (-2700)" → the message.
    let message = stderr
        .rsplit_once("Error: ")
        .map_or(stderr, |(_, message)| message);
    let message = message
        .rsplit_once(" (-")
        .map_or(message, |(message, _)| message);
    if message.is_empty() {
        "Music.app didn't answer".into()
    } else {
        message.to_string()
    }
}

fn javascript<T: for<'de> Deserialize<'de>>(source: &str, args: &[&str]) -> Result<T, String> {
    let output = osascript(Language::JavaScript, source, args)?;
    serde_json::from_str(&output).map_err(|error| format!("unexpected answer from Music: {error}"))
}

/// The whole library, read in one go.
pub fn snapshot() -> Result<super::library::Snapshot, String> {
    javascript(SNAPSHOT, &[])
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct NowPlaying {
    pub id: String,
    pub name: String,
    pub artist: String,
    pub album: String,
    /// Seconds.
    pub duration: f64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct PlayerState {
    pub running: bool,
    /// "playing", "paused", "stopped", "fast forwarding" or "rewinding".
    #[serde(default)]
    pub state: String,
    /// Seconds.
    #[serde(default)]
    pub position: f64,
    /// 0 to 100.
    #[serde(default)]
    pub volume: u8,
    #[serde(default)]
    pub shuffle: bool,
    /// "off", "one" or "all".
    #[serde(default)]
    pub repeat: String,
    #[serde(default)]
    pub track: Option<NowPlaying>,
    /// The persistent ID of the playlist playing from.
    #[serde(default)]
    pub playlist: Option<String>,
}

/// What Music.app is doing. Does not launch it.
pub fn state() -> Result<PlayerState, String> {
    javascript(STATE, &[])
}

#[derive(Debug, Deserialize)]
struct ControlAnswer {
    ok: bool,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    ids: Vec<String>,
}

fn control(args: &[&str]) -> Result<ControlAnswer, String> {
    let answer: ControlAnswer = javascript(CONTROL, args)?;
    if answer.ok {
        Ok(answer)
    } else {
        Err(answer.reason.unwrap_or_else(|| "Music refused".into()))
    }
}

pub fn toggle() -> Result<(), String> {
    control(&["toggle"]).map(drop)
}

pub fn next() -> Result<(), String> {
    control(&["next"]).map(drop)
}

pub fn previous() -> Result<(), String> {
    control(&["previous"]).map(drop)
}

pub fn pause() -> Result<(), String> {
    control(&["pause"]).map(drop)
}

pub fn seek(position_ms: u32) -> Result<(), String> {
    control(&["seek", &position_ms.to_string()]).map(drop)
}

pub fn volume(percent: u8) -> Result<(), String> {
    control(&["volume", &percent.min(100).to_string()]).map(drop)
}

pub fn shuffle(on: bool) -> Result<(), String> {
    control(&["shuffle", if on { "true" } else { "false" }]).map(drop)
}

/// `mode` is "off", "one" or "all".
pub fn repeat(mode: &str) -> Result<(), String> {
    control(&["repeat", mode]).map(drop)
}

/// Play a playlist from the top, or from one of its songs.
pub fn play_playlist(playlist: &str, track: Option<&str>) -> Result<(), String> {
    let mut args = vec!["play_playlist", playlist];
    args.extend(track);
    control(&args).map(drop)
}

/// Replace the queue playlist with these songs and play the first.
pub fn play_list(tracks: &[String]) -> Result<(), String> {
    let mut args = vec!["play_list"];
    args.extend(tracks.iter().map(String::as_str));
    control(&args).map(drop)
}

/// Append to the queue playlist. Refused unless it is what is playing:
/// Music.app's own Up Next cannot be scripted.
pub fn enqueue(tracks: &[String]) -> Result<(), String> {
    let mut args = vec!["enqueue"];
    args.extend(tracks.iter().map(String::as_str));
    control(&args).map(drop).map_err(|reason| {
        if reason == "not_queue" {
            "Music.app only lets Spotifast queue songs while it plays an album, an artist \
             or Liked Songs started here."
                .into()
        } else {
            reason
        }
    })
}

/// The songs after the current one in the queue playlist, by ID.
pub fn upcoming() -> Result<Vec<String>, String> {
    control(&["upcoming"]).map(|answer| answer.ids)
}

pub fn favorite(tracks: &[String], on: bool) -> Result<(), String> {
    let mut args = vec!["favorite", if on { "true" } else { "false" }];
    args.extend(tracks.iter().map(String::as_str));
    control(&args).map(drop)
}

/// A new playlist's persistent ID.
pub fn create_playlist(name: &str, description: &str) -> Result<String, String> {
    control(&["create_playlist", name, description])?
        .id
        .ok_or_else(|| "Music didn't say which playlist it made".into())
}

pub fn rename_playlist(playlist: &str, name: &str) -> Result<(), String> {
    control(&["rename_playlist", playlist, name]).map(drop)
}

pub fn describe_playlist(playlist: &str, description: &str) -> Result<(), String> {
    control(&["describe_playlist", playlist, description]).map(drop)
}

pub fn add_to_playlist(playlist: &str, tracks: &[String]) -> Result<(), String> {
    let mut args = vec!["add_to_playlist", playlist];
    args.extend(tracks.iter().map(String::as_str));
    control(&args).map(drop)
}

pub fn remove_from_playlist(playlist: &str, tracks: &[String]) -> Result<(), String> {
    let mut args = vec!["remove_from_playlist", playlist];
    args.extend(tracks.iter().map(String::as_str));
    control(&args).map(drop)
}

/// Write a library song's artwork to `path`. `Ok(false)` when it has none.
pub fn artwork(track: &str, path: &Path) -> Result<bool, String> {
    let path = path
        .to_str()
        .ok_or_else(|| "artwork path isn't valid text".to_string())?;
    let format = osascript(Language::AppleScript, ARTWORK, &[track, path])?;
    Ok(format != "none")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automation_refusal_is_explained() {
        let message = describe_failure(
            "execution error: Not authorized to send Apple events to Music. (-1743)",
        );
        assert!(message.contains("Automation"), "{message}");
    }

    #[test]
    fn script_errors_keep_only_their_message() {
        assert_eq!(
            describe_failure("0:10: execution error: Error: track not found: ABC (-2700)\n"),
            "track not found: ABC"
        );
    }

    #[test]
    fn stopped_state_parses_without_a_track() {
        let state: PlayerState = serde_json::from_str(
            r#"{"running":true,"state":"stopped","position":0,"volume":100,
                "shuffle":false,"repeat":"off","track":null,"playlist":null}"#,
        )
        .expect("state");
        assert!(state.running);
        assert_eq!(state.track, None);
    }

    #[test]
    fn closed_music_parses() {
        let state: PlayerState = serde_json::from_str(r#"{"running":false}"#).expect("state");
        assert!(!state.running);
    }
}
