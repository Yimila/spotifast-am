//! Answering the interface: Web API requests from the library, playback
//! commands through Music.app, and Music.app's player state back.

use super::*;

fn reloaded<T>(apple: &AppleMusic, result: Result<T, String>) -> Result<T, ApiError> {
    let value = result.map_err(music_error)?;
    apple.reload().map_err(music_error)?;
    Ok(value)
}

fn empty_page<T>(offset: u32) -> Page<T> {
    Page {
        items: Vec::new(),
        total: 0,
        limit: PAGE,
        offset,
        next: None,
    }
}

/// One request's answer, computed on the blocking pool.
fn answer(apple: &AppleMusic, request: ApiRequest) -> Option<ApiResponse> {
    use ApiRequest as R;
    use ApiResponse as A;
    Some(match request {
        R::Me => A::Me(apple.read(|library| library.user()).map_err(music_error)),
        R::Devices => A::Devices(Ok(Vec::new())),
        // Only this computer plays, and its state arrives as `Event::Local`.
        R::PlaybackState { seq } => A::PlaybackState {
            seq,
            result: Ok(None),
        },
        R::Queue { seq } => A::Queue {
            seq,
            result: bridge::upcoming().map_err(music_error).and_then(|ids| {
                let current = apple.lock().track_id.clone();
                apple
                    .read(|library| Queue {
                        currently_playing: current
                            .as_deref()
                            .and_then(|id| library.song(id))
                            .map(|song| PlayableItem::Track(library.track(song))),
                        queue: ids
                            .iter()
                            .filter_map(|id| library.song(id))
                            .map(|song| PlayableItem::Track(library.track(song)))
                            .collect(),
                    })
                    .map_err(music_error)
            }),
        },
        R::RecentlyPlayed {
            who,
            generation,
            limit,
            ..
        } => A::RecentlyPlayed {
            who,
            generation,
            limit,
            result: apple
                .read(|library| library.recently_played(limit))
                .map_err(music_error),
        },
        R::TopTracks {
            offset,
            full,
            generation,
        } => A::TopTracks {
            offset,
            full,
            generation,
            result: apple
                .read(|library| library.top_tracks(offset, PAGE))
                .map_err(music_error),
        },
        R::TopArtists { generation } => A::TopArtists {
            generation,
            result: apple.read(Library::top_artists).map_err(music_error),
        },
        R::Recommendations { generation, .. } => A::Recommendations {
            generation,
            result: apple.read(Library::recommendations).map_err(music_error),
        },
        R::Discover { term, generation } => A::Discover {
            term,
            generation,
            result: Ok(Vec::new()),
        },
        R::MyPlaylists { offset, generation } => {
            if offset == 0 {
                // A fresh library load: pick up changes made in Music.app.
                let _ = apple.reload();
            }
            A::MyPlaylists {
                offset,
                generation,
                result: apple
                    .read(|library| library.playlists(offset, PAGE))
                    .map_err(music_error),
            }
        }
        R::Playlist { id, generation } => A::Playlist {
            result: apple.find("That playlist", |library| library.playlist(&id)),
            id,
            generation,
        },
        R::PlaylistItems {
            id,
            offset,
            generation,
        } => A::PlaylistItems {
            result: apple.find("That playlist", |library| {
                library.playlist_items(&id, offset, PAGE)
            }),
            id,
            offset,
            generation,
        },
        R::PlaylistSample {
            id,
            offset,
            generation,
        } => A::PlaylistSample {
            result: apple.find("That playlist", |library| {
                library.playlist_items(&id, offset, PAGE)
            }),
            id,
            generation,
        },
        R::CreatePlaylist {
            name, description, ..
        } => A::PlaylistCreated(
            reloaded(apple, bridge::create_playlist(&name, &description))
                .and_then(|id| apple.find("The new playlist", |library| library.playlist(&id))),
        ),
        R::UploadPlaylistCover {
            id,
            request,
            previous_urls,
            cover,
        } => A::PlaylistCoverUploaded {
            id,
            request,
            previous_urls,
            cover,
            result: unsupported("Changing a playlist's cover"),
        },
        R::UpdatePlaylist {
            id,
            name,
            description,
            ..
        } => {
            let result = name
                .as_deref()
                .map_or(Ok(()), |name| bridge::rename_playlist(&id, name))
                .and_then(|()| {
                    description.as_deref().map_or(Ok(()), |description| {
                        bridge::describe_playlist(&id, description)
                    })
                });
            A::PlaylistUpdated {
                result: reloaded(apple, result),
                id,
            }
        }
        R::CheckPlaylistDuplicates {
            playlist_id,
            playlist_name,
            items,
            position,
        } => {
            let result = apple
                .find("That playlist", |library| {
                    library.list(&playlist_id).map(|list| list.tracks.clone())
                })
                .map(|present| {
                    items
                        .iter()
                        .map(|item| item.uri().to_string())
                        .filter(|uri| {
                            crate::util::uri_id(uri)
                                .is_some_and(|id| present.iter().any(|known| known == id))
                        })
                        .collect()
                });
            A::PlaylistDuplicatesChecked {
                playlist_id,
                playlist_name,
                items,
                position,
                result,
            }
        }
        R::AddToPlaylist {
            playlist_id,
            playlist_name,
            uris,
            ..
        } => {
            let result = reloaded(
                apple,
                bridge::add_to_playlist(&playlist_id, &library::track_ids(&uris)),
            )
            .and_then(|()| {
                apple.find("That playlist", |library| {
                    library.playlist(&playlist_id).map(|list| list.snapshot_id)
                })
            });
            A::PlaylistItemsChanged {
                id: playlist_id,
                message: format!("Added to {playlist_name}"),
                result,
            }
        }
        R::RemoveFromPlaylist {
            playlist_id, uris, ..
        } => {
            let result = reloaded(
                apple,
                bridge::remove_from_playlist(&playlist_id, &library::track_ids(&uris)),
            )
            .and_then(|()| {
                apple.find("That playlist", |library| {
                    library.playlist(&playlist_id).map(|list| list.snapshot_id)
                })
            });
            A::PlaylistItemsChanged {
                id: playlist_id,
                message: "Removed from playlist".into(),
                result,
            }
        }
        R::ReorderPlaylist { playlist_id, .. } => A::PlaylistItemsChanged {
            id: playlist_id,
            message: String::new(),
            result: unsupported("Reordering a playlist"),
        },
        R::FollowPlaylist { id, follow } => A::PlaylistFollowChanged {
            id,
            followed: follow,
            result: unsupported("Following playlists"),
        },
        R::SavedTracks { offset, generation } => A::SavedTracks {
            offset,
            generation,
            account_id: Some(library::ACCOUNT_ID.into()),
            result: apple
                .read(|library| library.saved_tracks(offset, PAGE))
                .map_err(music_error),
        },
        R::SavedAlbums { offset } => A::SavedAlbums {
            offset,
            result: apple
                .read(|library| library.saved_albums(offset, PAGE))
                .map_err(music_error),
        },
        R::FollowedArtists { after } => A::FollowedArtists {
            result: apple
                .read(|library| library.followed_artists(after.as_deref(), PAGE))
                .map_err(music_error),
            after,
        },
        R::SavedShows { offset } => A::SavedShows {
            offset,
            result: Ok(empty_page(offset)),
        },
        R::SavedEpisodes { offset } => A::SavedEpisodes {
            offset,
            result: Ok(empty_page(offset)),
        },
        R::SetSaved { uris, saved } => {
            let ids = library::track_ids(&uris);
            let result = bridge::favorite(&ids, saved).map_err(music_error);
            if result.is_ok() {
                let mut shared = apple.lock();
                if let Some(library) = shared.library.as_mut() {
                    for id in &ids {
                        library.set_favorited(id, saved);
                    }
                }
            }
            A::SavedChanged {
                uris,
                saved,
                result,
            }
        }
        R::Contains { uris } => A::Contains {
            result: apple
                .read(|library| library.contains(&uris))
                .map_err(music_error),
            uris,
        },
        R::Search { query, serial }
        | R::SearchCatalogue { query, serial }
        | R::SearchPlaylists { query, serial } => A::Search {
            result: apple
                .read(|library| library.search(&query, 20))
                .map_err(music_error),
            query,
            serial,
        },
        R::Artist { id } => A::Artist {
            result: apple.find("That artist", |library| library.artist(&id)),
            id,
        },
        R::ArtistTopTracks { id } => A::ArtistTopTracks {
            result: apple.find("That artist", |library| library.artist_top_tracks(&id)),
            id,
        },
        R::ArtistAlbums { id, groups, offset } => A::ArtistAlbums {
            result: apple.find("That artist", |library| {
                library.artist_albums(&id, offset, PAGE)
            }),
            id,
            groups,
            offset,
        },
        R::RelatedArtists { id } => A::RelatedArtists {
            result: apple.find("That artist", |library| library.related_artists(&id)),
            id,
        },
        R::Album { id } => A::Album {
            result: apple.find("That album", |library| library.album(&id)),
            id,
        },
        R::AlbumTracks {
            id,
            offset,
            generation,
        } => A::AlbumTracks {
            result: apple.find("That album", |library| {
                library.album_tracks(&id, offset, PAGE)
            }),
            id,
            offset,
            generation,
        },
        R::AlbumQueueTracks {
            id,
            offset,
            request,
        } => A::AlbumQueueTracks {
            result: apple.find("That album", |library| {
                library.album_tracks(&id, offset, PAGE)
            }),
            offset,
            request,
        },
        R::Show { id } => A::Show {
            id,
            result: unsupported("Podcasts"),
        },
        R::ShowEpisodes { id, offset } => A::ShowEpisodes {
            id,
            offset,
            result: unsupported("Podcasts"),
        },
        R::HomeEpisodes { generation, .. } => A::HomeEpisodes {
            generation,
            result: Ok(Vec::new()),
        },
        R::Track { id } => A::Track {
            result: apple.find("That song", |library| {
                library.song(&id).map(|song| library.track(song))
            }),
            id,
        },
        R::Episode { id } => A::Episode {
            id,
            result: unsupported("Podcasts"),
        },
        R::Remote { action, .. } => A::Remote {
            action,
            result: unsupported("Controlling other devices"),
        },
        R::Transfer { device_id, .. } => A::Transferred {
            device_id,
            result: unsupported("Controlling other devices"),
        },
        R::ShufflePlay { play, .. } => {
            apple.load(&LoadSpec {
                context_uri: play.context_uri,
                uris: play.uris,
                offset_uri: play.offset_uri,
                offset_index: play.offset_position,
                play: true,
                shuffle: Some(true),
                ..LoadSpec::default()
            });
            return None;
        }
        R::AddToQueue { uri, label, .. } => A::QueueAdded {
            label,
            result: bridge::enqueue(&library::track_ids(&[uri])).map_err(music_error),
        },
        R::AddManyToQueue { request, uris, .. } => {
            let ids = library::track_ids(&uris);
            let result = bridge::enqueue(&ids).map_err(music_error);
            A::QueueBatchAdded {
                request,
                added: if result.is_ok() { ids.len() } else { 0 },
                result,
            }
        }
    })
}

impl AppleMusic {
    pub(super) fn api(&self, request: ApiRequest) {
        if let ApiRequest::Search { query, serial } = &request {
            if query.is_empty() {
                return;
            }
            self.emit(Event::Api(Box::new(ApiResponse::SearchStarted {
                query: query.clone(),
                serial: *serial,
                split: false,
            })));
        }
        self.blocking(move |apple| {
            answer(apple, request).map(|response| Event::Api(Box::new(response)))
        });
    }

    pub(super) fn player(&self, command: PlayerCommand) {
        self.blocking(move |apple| {
            let result = match command {
                PlayerCommand::Toggle => bridge::toggle(),
                PlayerCommand::Next => bridge::next(),
                PlayerCommand::Previous => bridge::previous(),
                PlayerCommand::ClearQueue | PlayerCommand::Transfer => Ok(()),
                PlayerCommand::AddToQueue(uri) => bridge::enqueue(&library::track_ids(&[uri])),
                PlayerCommand::Seek(position_ms) => {
                    let result = bridge::seek(position_ms);
                    apple.lock().seek_sequence += 1;
                    result
                }
                PlayerCommand::Volume(volume) | PlayerCommand::VolumePreview(volume) => {
                    bridge::volume(percent(volume))
                }
                PlayerCommand::Shuffle(on) => bridge::shuffle(on),
                PlayerCommand::Repeat(mode) => bridge::repeat(repeat_name(mode)),
                PlayerCommand::Load(spec) => {
                    apple.load(&spec);
                    Ok(())
                }
            };
            apple.poll_now.notify_one();
            result
                .err()
                .map(|error| Event::Error(format!("Music: {error}")))
        });
    }

    /// Start what the interface asked to play. Blocking.
    fn load(&self, spec: &LoadSpec) {
        if let Err(error) = self.try_load(spec) {
            self.emit(Event::Error(format!("Music: {error}")));
        }
        self.poll_now.notify_one();
    }

    fn try_load(&self, spec: &LoadSpec) -> Result<(), String> {
        if let Some(on) = spec.shuffle {
            bridge::shuffle(on)?;
        }
        if let Some(mode) = spec.repeat {
            bridge::repeat(repeat_name(mode))?;
        }
        let context = spec.context_uri.as_deref().unwrap_or_default();
        let offset_id = spec
            .offset_uri
            .as_deref()
            .filter(|uri| crate::util::uri_kind(uri) == Some("track"))
            .and_then(crate::util::uri_id)
            .map(str::to_string);

        if let Some(playlist) = context.strip_prefix("spotify:playlist:") {
            let start = offset_id.or_else(|| {
                let index = spec.offset_index? as usize;
                self.read(|library| library.list(playlist)?.tracks.get(index).cloned())
                    .ok()
                    .flatten()
            });
            bridge::play_playlist(playlist, start.as_deref())?;
        } else {
            let ids = if context.ends_with(":collection") {
                self.read(Library::liked_ids)?
            } else if let Some(album) = context.strip_prefix("spotify:album:") {
                self.read(|library| library.album_track_ids(album))?
                    .ok_or("That album isn't in your Music library")?
            } else if let Some(artist) = context.strip_prefix("spotify:artist:") {
                self.read(|library| library.artist_track_ids(artist))?
                    .ok_or("That artist isn't in your Music library")?
            } else if !spec.uris.is_empty() {
                library::track_ids(&spec.uris)
            } else {
                return Err(format!("Music can't play {context}"));
            };
            bridge::play_list(&starting_at(ids, offset_id.as_deref(), spec.offset_index))?;
        }
        if spec.position_ms > 0 {
            bridge::seek(spec.position_ms)?;
        }
        if !spec.play {
            bridge::pause()?;
        }
        Ok(())
    }

    pub(super) fn spawn_poller(&self) {
        let apple = self.handle_clone();
        tokio::spawn(async move {
            loop {
                let state = tokio::task::spawn_blocking(bridge::state).await;
                let playing = match state {
                    Ok(Ok(state)) => apple.observe(state),
                    _ => false,
                };
                let wait = if playing { POLL_PLAYING } else { POLL_IDLE };
                tokio::select! {
                    () = tokio::time::sleep(wait) => {}
                    () = apple.poll_now.notified() => {}
                }
            }
        });
    }

    /// Turn Music.app's state into the interface's, and send it on when
    /// something the interface shows has changed. True while playing.
    fn observe(&self, state: bridge::PlayerState) -> bool {
        let now = Instant::now();
        let mut shared = self.lock();
        let playback = match state.state.as_str() {
            _ if !state.running => Playback::Stopped,
            "playing" | "fast forwarding" | "rewinding" => Playback::Playing,
            "paused" => Playback::Paused,
            _ => Playback::Stopped,
        };
        let track_id = state.track.as_ref().map(|track| track.id.clone());
        if track_id.is_some() && track_id != shared.track_id {
            shared.track_sequence += 1;
        }
        shared.track_id.clone_from(&track_id);
        let track = state.track.as_ref().map(|track| {
            let artists = shared
                .library
                .as_ref()
                .and_then(|library| library.song(&track.id).map(|song| library.track(song)))
                .map(|known| known.artists)
                .filter(|artists| !artists.is_empty())
                .unwrap_or_else(|| {
                    vec![ArtistRef {
                        name: track.artist.clone(),
                        ..ArtistRef::default()
                    }]
                });
            LocalTrack {
                uri: library::track_uri(&track.id),
                title: track.name.clone(),
                artists,
                album: track.album.clone(),
                art_url: Some(library::art_url(&track.id)),
                art_small_url: Some(library::art_url(&track.id)),
                duration_ms: (track.duration * 1000.0).round() as u32,
                is_episode: false,
            }
        });
        let next = LocalState {
            playback,
            track,
            position_ms: (state.position * 1000.0).round() as u32,
            position_at: (playback == Playback::Playing).then_some(now),
            volume: (u32::from(state.volume.min(100)) * u32::from(u16::MAX) / 100) as u16,
            shuffle: state.shuffle,
            repeat: match state.repeat.as_str() {
                "one" => RepeatMode::Track,
                "all" => RepeatMode::Context,
                _ => RepeatMode::Off,
            },
            connected: true,
            username: shared.local.username.clone(),
            active_client: "Music".into(),
            error: None,
            seek_sequence: shared.seek_sequence,
            track_sequence: shared.track_sequence,
            replay_pending: false,
        };
        if changed(&shared.local, &next, now) {
            shared.local = next.clone();
            drop(shared);
            self.emit(Event::Local(Box::new(next)));
        }
        playback == Playback::Playing
    }
}

/// Whether the interface needs `next`: anything it shows changed, or the
/// position moved further than the clock alone explains.
fn changed(last: &LocalState, next: &LocalState, now: Instant) -> bool {
    let expected = match last.position_at {
        Some(at) => i64::from(last.position_ms) + now.duration_since(at).as_millis() as i64,
        None => i64::from(last.position_ms),
    };
    last.playback != next.playback
        || last.track != next.track
        || last.volume != next.volume
        || last.shuffle != next.shuffle
        || last.repeat != next.repeat
        || last.seek_sequence != next.seek_sequence
        || last.track_sequence != next.track_sequence
        || (i64::from(next.position_ms) - expected).abs() > POSITION_DRIFT_MS
}

/// The songs from the chosen one on, then the ones before it, so a list
/// started midway still plays through.
fn starting_at(
    ids: Vec<String>,
    offset_id: Option<&str>,
    offset_index: Option<u32>,
) -> Vec<String> {
    let start = offset_id
        .and_then(|id| ids.iter().position(|candidate| candidate == id))
        .or(offset_index.map(|index| index as usize))
        .unwrap_or(0)
        .min(ids.len());
    let mut ordered = ids[start..].to_vec();
    ordered.extend_from_slice(&ids[..start]);
    ordered.truncate(MAX_QUEUE);
    ordered
}

/// librespot's volume scale, 0 to 65535, as Music.app's percentage.
fn percent(volume: u16) -> u8 {
    ((u32::from(volume) * 100 + u32::from(u16::MAX) / 2) / u32::from(u16::MAX)) as u8
}

fn repeat_name(mode: RepeatMode) -> &'static str {
    match mode {
        RepeatMode::Off => "off",
        RepeatMode::Context => "all",
        RepeatMode::Track => "one",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_scales_to_a_percentage() {
        assert_eq!(percent(0), 0);
        assert_eq!(percent(u16::MAX), 100);
        assert_eq!(percent(u16::MAX / 2), 50);
    }

    #[test]
    fn repeat_modes_match_music() {
        assert_eq!(repeat_name(RepeatMode::Context), "all");
        assert_eq!(repeat_name(RepeatMode::Track), "one");
        assert_eq!(repeat_name(RepeatMode::Off), "off");
    }

    #[test]
    fn a_list_started_midway_wraps_around() {
        let ids: Vec<String> = ["a", "b", "c"].map(String::from).to_vec();
        assert_eq!(starting_at(ids.clone(), Some("b"), None), ["b", "c", "a"]);
        assert_eq!(starting_at(ids.clone(), None, Some(2)), ["c", "a", "b"]);
        assert_eq!(starting_at(ids.clone(), None, Some(9)), ["a", "b", "c"]);
        assert_eq!(starting_at(ids, Some("missing"), None), ["a", "b", "c"]);
    }

    #[test]
    fn steady_playback_is_not_resent() {
        let at = Instant::now();
        let last = LocalState {
            playback: Playback::Playing,
            position_ms: 10_000,
            position_at: Some(at),
            ..LocalState::default()
        };
        let later = at + Duration::from_secs(1);
        let next = LocalState {
            position_ms: 11_000,
            position_at: Some(later),
            ..last.clone()
        };
        assert!(!changed(&last, &next, later));
        let jumped = LocalState {
            position_ms: 40_000,
            ..next.clone()
        };
        assert!(changed(&last, &jumped, later));
        let paused = LocalState {
            playback: Playback::Paused,
            ..next
        };
        assert!(changed(&last, &paused, later));
    }
}
