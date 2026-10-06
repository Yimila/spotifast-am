//! Apple Music through Music.app.
//!
//! Apple Music audio is protected by FairPlay and only Apple's own players
//! can decode it, so Spotifast does not stream it. Instead it drives
//! Music.app over Apple Events: the library and playlists are read from
//! it, playback commands are sent to it, and its player state is polled
//! back into the `LocalState` the interface already draws.
//!
//! This module answers the backend's Spotify-facing commands (sign-in, Web
//! API requests, the playback engine, Connect) in place of the Spotify
//! worker; everything else (lyrics, artwork, updates, themes) is still
//! handled by the worker in `backend.rs`.

mod answers;
pub mod bridge;
pub mod library;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::api::ApiError;
use crate::api::models::*;
use crate::backend::{ApiRequest, ApiResponse, AuthStatus, Command, Event, LocalPlayback, Waker};
use crate::player::{
    LoadSpec, LocalState, LocalTrack, Playback, PlayerCommand, RepeatMode, Rootlist,
};
use library::Library;

/// The device ID the interface knows this computer's player by.
pub const DEVICE_ID: &str = "music-app";
const PAGE: u32 = 50;
/// The most songs copied into the queue playlist for one play. Each copy
/// is an Apple Event of its own.
const MAX_QUEUE: usize = 300;
const POLL_PLAYING: Duration = Duration::from_millis(1000);
const POLL_IDLE: Duration = Duration::from_millis(2500);
/// How far the reported position may drift from the extrapolated one
/// before the interface is told again.
const POSITION_DRIFT_MS: i64 = 1500;

/// Whether the Apple Music backend replaces Spotify in this build and run.
/// Set `SPOTIFAST_BACKEND=spotify` to use the original Spotify backend.
pub fn enabled() -> bool {
    cfg!(all(target_os = "macos", not(test)))
        && std::env::var("SPOTIFAST_BACKEND").map_or(true, |value| value != "spotify")
}

fn unsupported<T>(what: &str) -> Result<T, ApiError> {
    Err(ApiError::Status {
        status: 501,
        message: format!("{what} isn't available with Apple Music"),
    })
}

fn music_error(message: String) -> ApiError {
    ApiError::Status {
        status: 502,
        message,
    }
}

#[derive(Default)]
struct Shared {
    library: Option<Library>,
    /// What the interface was last told, to send only real changes.
    local: LocalState,
    track_id: Option<String>,
    track_sequence: u64,
    seek_sequence: u64,
}

pub struct AppleMusic {
    shared: Arc<Mutex<Shared>>,
    events: std::sync::mpsc::Sender<Event>,
    waker: Waker,
    poll_now: Arc<tokio::sync::Notify>,
}

impl AppleMusic {
    pub fn start(events: std::sync::mpsc::Sender<Event>, waker: Waker) -> Self {
        let apple = Self {
            shared: Arc::new(Mutex::new(Shared::default())),
            events,
            waker,
            poll_now: Arc::new(tokio::sync::Notify::new()),
        };
        apple.announce();
        apple.spawn_poller();
        apple
    }

    fn emit(&self, event: Event) {
        let _ = self.events.send(event);
        self.waker.wake();
    }

    /// Signed in and ready to play, as far as the interface can tell: there
    /// is no account to sign in to.
    fn announce(&self) {
        let user = Library::default().user();
        self.emit(Event::Auth(AuthStatus::Connected {
            username: user.name().to_string(),
        }));
        self.emit(Event::Api(Box::new(ApiResponse::Me(Ok(user)))));
        self.emit(Event::Playback(LocalPlayback::Ready {
            device_id: DEVICE_ID.into(),
        }));
        let local = LocalState {
            connected: true,
            username: Library::default().user().name().to_string(),
            active_client: "Music".into(),
            volume: u16::MAX,
            ..LocalState::default()
        };
        self.lock().local = local.clone();
        self.emit(Event::Local(Box::new(local)));
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Takes the commands this backend answers and hands back the rest.
    pub fn handle(&self, command: Command) -> Option<Command> {
        match command {
            Command::Api(request) => self.api(request),
            Command::Player(command) => self.player(command),
            Command::SignIn { .. } | Command::SignOut | Command::CancelSignIn => self.announce(),
            Command::AuthorizePlayback | Command::Reconnect | Command::VerifyResume => {
                self.emit(Event::Playback(LocalPlayback::Ready {
                    device_id: DEVICE_ID.into(),
                }));
            }
            Command::RestartEngine(_) => {}
            Command::Rootlist => self.blocking(|apple| {
                let result = apple.read(|library| Rootlist {
                    entries: library.rootlist(),
                    editable: library.editable_playlists().collect(),
                });
                Some(Event::Rootlist { result })
            }),
            Command::LoadPlaylistCache { id, generation } => self.emit(Event::PlaylistCache {
                account_id: library::ACCOUNT_ID.into(),
                id,
                generation,
                cache: None,
            }),
            Command::StorePlaylistCache { .. } | Command::StoreLikedSongsCache(_) => {}
            Command::LoadLikedSongsCache { generation } => self.emit(Event::LikedSongsCache {
                account_id: library::ACCOUNT_ID.into(),
                generation,
                cache: None,
            }),
            Command::UserNames(_) | Command::AlbumTypes(_) => {}
            Command::AudiobookShows(_) => self.emit(Event::AudiobookShows(Vec::new())),
            Command::Radio { seed, generation } => self.emit(Event::Radio {
                seed,
                generation,
                result: Err("Radio isn't available with Apple Music".into()),
            }),
            Command::DiscoverReceivers => self.emit(Event::Receivers(Vec::new())),
            Command::ActivateReceiver(receiver) => self.emit(Event::ReceiverActivated {
                name: receiver.name.clone(),
                result: Err("Spotify Connect isn't available with Apple Music".into()),
            }),
            Command::ConfigurePersonalWebApp(_) => self.emit(Event::WebApp { client_id: None }),
            other => return Some(other),
        }
        None
    }

    /// Run `work` on the blocking pool, where talking to Music.app may
    /// wait, and emit what it answers.
    fn blocking(&self, work: impl FnOnce(&AppleMusic) -> Option<Event> + Send + 'static) {
        let apple = self.handle_clone();
        tokio::task::spawn_blocking(move || {
            if let Some(event) = work(&apple) {
                apple.emit(event);
            }
        });
    }

    /// Read from the library, loading it first if need be. Blocking.
    fn read<T>(&self, read: impl FnOnce(&Library) -> T) -> Result<T, String> {
        self.ensure_library()?;
        let shared = self.lock();
        let library = shared
            .library
            .as_ref()
            .ok_or_else(|| "The Music library isn't loaded".to_string())?;
        Ok(read(library))
    }

    /// Like `read`, as an API result: a missing item is a 404.
    fn find<T>(&self, what: &str, read: impl FnOnce(&Library) -> Option<T>) -> Result<T, ApiError> {
        self.read(read)
            .map_err(music_error)?
            .ok_or_else(|| ApiError::Status {
                status: 404,
                message: format!("{what} isn't in your Music library"),
            })
    }

    fn handle_clone(&self) -> AppleMusic {
        AppleMusic {
            shared: Arc::clone(&self.shared),
            events: self.events.clone(),
            waker: self.waker.clone(),
            poll_now: Arc::clone(&self.poll_now),
        }
    }

    fn ensure_library(&self) -> Result<(), String> {
        if self.lock().library.is_some() {
            return Ok(());
        }
        self.reload()
    }

    /// Read the library again, after a change or on first use.
    fn reload(&self) -> Result<(), String> {
        let library = Library::from_snapshot(bridge::snapshot()?);
        self.lock().library = Some(library);
        Ok(())
    }
}
