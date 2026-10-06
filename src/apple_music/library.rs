//! The Music.app library, held in memory and shaped like the Web API's
//! answers so the interface can draw it unchanged.
//!
//! Music.app has songs and playlists; albums and artists are derived here
//! from the songs' tags. Every item keeps the `spotify:<kind>:<id>` URI
//! shape the interface parses, with Music.app persistent IDs for songs and
//! playlists and stable hashes of the tags for albums and artists.

#[cfg(test)]
use std::collections::BTreeMap;
use std::collections::HashMap;

use serde::Deserialize;
use sha1::{Digest, Sha1};

use crate::api::models::*;
use crate::player::RootlistEntry;

/// The account ID every Apple Music answer carries. Playlists owned by it
/// are editable in the interface.
pub const ACCOUNT_ID: &str = "apple-music";

/// Artwork URL for a library song, fetched by `ArtLoader` through Music.app.
pub const ART_SCHEME: &str = "musicart://";

pub fn art_url(track_id: &str) -> String {
    format!("{ART_SCHEME}{track_id}")
}

/// The song ID of an artwork URL made by `art_url`.
pub fn art_track(url: &str) -> Option<&str> {
    url.strip_prefix(ART_SCHEME)
        .filter(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric()))
}

pub fn track_uri(id: &str) -> String {
    format!("spotify:track:{id}")
}

fn album_uri(id: &str) -> String {
    format!("spotify:album:{id}")
}

fn artist_uri(id: &str) -> String {
    format!("spotify:artist:{id}")
}

pub fn playlist_uri(id: &str) -> String {
    format!("spotify:playlist:{id}")
}

pub fn collection_uri() -> String {
    format!("spotify:user:{ACCOUNT_ID}:collection")
}

/// A short, stable ID for something known only by its tags.
fn tag_id(kind: &str, parts: &[&str]) -> String {
    let mut digest = Sha1::new();
    digest.update(kind.as_bytes());
    for part in parts {
        digest.update([0]);
        digest.update(part.to_lowercase().as_bytes());
    }
    digest.finalize()[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The snapshot script's answer: one array per property, row-aligned.
#[derive(Debug, Default, Deserialize)]
pub struct Snapshot {
    pub tracks: TrackColumns,
    pub playlists: PlaylistColumns,
}

#[derive(Debug, Default, Deserialize)]
pub struct TrackColumns {
    pub id: Vec<String>,
    pub name: Vec<String>,
    pub artist: Vec<String>,
    pub album: Vec<String>,
    pub album_artist: Vec<String>,
    pub duration: Vec<Option<f64>>,
    pub track_number: Vec<Option<u32>>,
    pub disc_number: Vec<Option<u32>>,
    pub year: Vec<Option<u32>>,
    pub genre: Vec<String>,
    pub favorited: Vec<Option<bool>>,
    pub played_count: Vec<Option<u32>>,
    pub played_date: Vec<Option<String>>,
    pub date_added: Vec<Option<String>>,
}

#[derive(Debug, Default, Deserialize)]
pub struct PlaylistColumns {
    pub id: Vec<String>,
    pub name: Vec<String>,
    pub description: Vec<Option<String>>,
    pub smart: Vec<Option<bool>>,
    pub special: Vec<String>,
    pub class: Vec<String>,
    pub parent: Vec<Option<String>>,
    pub tracks: Vec<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Song {
    pub id: String,
    pub name: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub duration_ms: u32,
    pub track_number: u32,
    pub disc_number: u32,
    pub year: u32,
    pub genre: String,
    pub favorited: bool,
    pub played_count: u32,
    pub played_date: Option<String>,
    pub date_added: Option<String>,
}

impl Song {
    /// Who the album is filed under.
    fn album_owner(&self) -> &str {
        if self.album_artist.is_empty() {
            &self.artist
        } else {
            &self.album_artist
        }
    }

    fn album_id(&self) -> Option<String> {
        (!self.album.is_empty()).then(|| tag_id("album", &[self.album_owner(), &self.album]))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct List {
    pub id: String,
    pub name: String,
    pub description: String,
    pub smart: bool,
    pub folder: bool,
    pub parent: Option<String>,
    pub tracks: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct AlbumEntry {
    id: String,
    name: String,
    artist: String,
    /// Song indexes in disc and track order.
    songs: Vec<usize>,
    year: u32,
    genre: String,
    newest_added: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct ArtistEntry {
    id: String,
    name: String,
    songs: Vec<usize>,
    albums: Vec<usize>,
    played: u64,
}

#[derive(Debug, Default)]
pub struct Library {
    songs: Vec<Song>,
    by_id: HashMap<String, usize>,
    albums: Vec<AlbumEntry>,
    album_index: HashMap<String, usize>,
    artists: Vec<ArtistEntry>,
    artist_index: HashMap<String, usize>,
    lists: Vec<List>,
}

fn column<T: Clone + Default>(values: &[T], index: usize) -> T {
    values.get(index).cloned().unwrap_or_default()
}

impl Library {
    pub fn from_snapshot(snapshot: Snapshot) -> Self {
        let columns = &snapshot.tracks;
        let songs: Vec<Song> = (0..columns.id.len())
            .map(|index| Song {
                id: columns.id[index].clone(),
                name: column(&columns.name, index),
                artist: column(&columns.artist, index),
                album: column(&columns.album, index),
                album_artist: column(&columns.album_artist, index),
                duration_ms: column(&columns.duration, index)
                    .map_or(0, |seconds| (seconds * 1000.0).round() as u32),
                track_number: column(&columns.track_number, index).unwrap_or(0),
                disc_number: column(&columns.disc_number, index).unwrap_or(0),
                year: column(&columns.year, index).unwrap_or(0),
                genre: column(&columns.genre, index),
                favorited: column(&columns.favorited, index).unwrap_or(false),
                played_count: column(&columns.played_count, index).unwrap_or(0),
                played_date: column(&columns.played_date, index),
                date_added: column(&columns.date_added, index),
            })
            .collect();

        let lists = {
            let columns = &snapshot.playlists;
            (0..columns.id.len())
                // "Music", "Movies" and the like are views of the library,
                // not playlists. The queue playlist is Spotifast's own.
                .filter(|&index| {
                    matches!(
                        columns.special.get(index).map(String::as_str),
                        None | Some("none")
                    ) && column(&columns.name, index) != super::bridge::QUEUE_PLAYLIST
                })
                .map(|index| List {
                    id: columns.id[index].clone(),
                    name: column(&columns.name, index),
                    description: column(&columns.description, index).unwrap_or_default(),
                    smart: column(&columns.smart, index).unwrap_or(false),
                    folder: column(&columns.class, index) == "folderPlaylist",
                    parent: column(&columns.parent, index),
                    tracks: column(&columns.tracks, index),
                })
                .collect()
        };
        Self::new(songs, lists)
    }

    pub fn new(songs: Vec<Song>, lists: Vec<List>) -> Self {
        let by_id = songs
            .iter()
            .enumerate()
            .map(|(index, song)| (song.id.clone(), index))
            .collect();

        let mut album_index: HashMap<String, usize> = HashMap::new();
        let mut albums: Vec<AlbumEntry> = Vec::new();
        for (index, song) in songs.iter().enumerate() {
            let Some(id) = song.album_id() else { continue };
            let slot = *album_index.entry(id.clone()).or_insert_with(|| {
                albums.push(AlbumEntry {
                    id,
                    name: song.album.clone(),
                    artist: song.album_owner().to_string(),
                    songs: Vec::new(),
                    year: 0,
                    genre: String::new(),
                    newest_added: None,
                });
                albums.len() - 1
            });
            let album = &mut albums[slot];
            album.songs.push(index);
            album.year = album.year.max(song.year);
            if album.genre.is_empty() {
                album.genre = song.genre.clone();
            }
            if song.date_added > album.newest_added {
                album.newest_added = song.date_added.clone();
            }
        }
        for album in &mut albums {
            album.songs.sort_by_key(|&index| {
                let song = &songs[index];
                (
                    song.disc_number,
                    song.track_number,
                    song.name.to_lowercase(),
                )
            });
        }

        let mut artist_index: HashMap<String, usize> = HashMap::new();
        let mut artists: Vec<ArtistEntry> = Vec::new();
        let mut artist_slot = |name: &str, artists: &mut Vec<ArtistEntry>| -> Option<usize> {
            if name.is_empty() {
                return None;
            }
            let id = tag_id("artist", &[name]);
            Some(*artist_index.entry(id.clone()).or_insert_with(|| {
                artists.push(ArtistEntry {
                    id,
                    name: name.to_string(),
                    songs: Vec::new(),
                    albums: Vec::new(),
                    played: 0,
                });
                artists.len() - 1
            }))
        };
        for (index, song) in songs.iter().enumerate() {
            if let Some(slot) = artist_slot(&song.artist, &mut artists) {
                artists[slot].songs.push(index);
                artists[slot].played += u64::from(song.played_count);
            }
            if song.album_artist != song.artist
                && let Some(slot) = artist_slot(&song.album_artist, &mut artists)
            {
                artists[slot].songs.push(index);
            }
        }
        for (album_slot, album) in albums.iter().enumerate() {
            if let Some(slot) = artist_slot(&album.artist, &mut artists) {
                artists[slot].albums.push(album_slot);
            }
        }
        for artist in &mut artists {
            artist.albums.sort_by(|a, b| {
                albums[*b]
                    .year
                    .cmp(&albums[*a].year)
                    .then_with(|| albums[*a].name.cmp(&albums[*b].name))
            });
        }

        Self {
            songs,
            by_id,
            albums,
            album_index,
            artists,
            artist_index,
            lists,
        }
    }

    pub fn song(&self, id: &str) -> Option<&Song> {
        self.by_id.get(id).map(|&index| &self.songs[index])
    }

    pub fn list(&self, id: &str) -> Option<&List> {
        self.lists.iter().find(|list| list.id == id)
    }

    pub fn set_favorited(&mut self, id: &str, favorited: bool) {
        if let Some(&index) = self.by_id.get(id) {
            self.songs[index].favorited = favorited;
        }
    }

    // ----- Models -----

    fn artist_ref(&self, name: &str) -> ArtistRef {
        let id = tag_id("artist", &[name]);
        ArtistRef {
            uri: Some(artist_uri(&id)),
            id: Some(id),
            name: name.to_string(),
        }
    }

    fn album_model(&self, album: &AlbumEntry) -> Album {
        let first = album.songs.first().map(|&index| &self.songs[index]);
        Album {
            uri: album_uri(&album.id),
            id: album.id.clone(),
            name: album.name.clone(),
            album_type: Some(
                if album.songs.len() <= 3 {
                    "single"
                } else {
                    "album"
                }
                .into(),
            ),
            total_tracks: Some(album.songs.len() as u32),
            images: first.map(|song| image(&song.id)).into_iter().collect(),
            artists: vec![self.artist_ref(&album.artist)],
            release_date: (album.year > 0).then(|| album.year.to_string()),
            genres: (!album.genre.is_empty())
                .then(|| album.genre.clone())
                .into_iter()
                .collect(),
            ..Album::default()
        }
    }

    pub fn track(&self, song: &Song) -> Track {
        let album = song
            .album_id()
            .and_then(|id| self.album_index.get(&id))
            .map(|&slot| Album {
                tracks: None,
                ..self.album_model(&self.albums[slot])
            })
            .or_else(|| {
                Some(Album {
                    images: vec![image(&song.id)],
                    ..Album::default()
                })
            });
        Track {
            id: Some(song.id.clone()),
            uri: track_uri(&song.id),
            name: song.name.clone(),
            duration_ms: song.duration_ms,
            artists: (!song.artist.is_empty())
                .then(|| self.artist_ref(&song.artist))
                .into_iter()
                .collect(),
            album,
            track_number: (song.track_number > 0).then_some(song.track_number),
            disc_number: (song.disc_number > 0).then_some(song.disc_number),
            is_playable: Some(true),
            ..Track::default()
        }
    }

    fn artist_model(&self, artist: &ArtistEntry) -> Artist {
        let cover = artist
            .albums
            .first()
            .and_then(|&slot| self.albums[slot].songs.first())
            .or(artist.songs.first())
            .map(|&index| image(&self.songs[index].id));
        let mut genres: Vec<String> = artist
            .songs
            .iter()
            .map(|&index| self.songs[index].genre.clone())
            .filter(|genre| !genre.is_empty())
            .collect();
        genres.sort();
        genres.dedup();
        Artist {
            uri: artist_uri(&artist.id),
            id: artist.id.clone(),
            name: artist.name.clone(),
            images: cover.into_iter().collect(),
            genres,
            ..Artist::default()
        }
    }

    pub fn user(&self) -> User {
        let name = std::env::var("USER").unwrap_or_else(|_| "Apple Music".into());
        User {
            id: ACCOUNT_ID.into(),
            display_name: Some(name),
            // No Spotify plan: neither the Premium notice nor the personal
            // Spotify app suggestion applies.
            product: None,
            uri: Some(format!("spotify:user:{ACCOUNT_ID}")),
            ..User::default()
        }
    }

    fn playlist_model(&self, list: &List) -> Playlist {
        let cover = list
            .tracks
            .iter()
            .find(|id| self.by_id.contains_key(*id))
            .map(|id| image(id));
        Playlist {
            id: list.id.clone(),
            uri: playlist_uri(&list.id),
            name: list.name.clone(),
            description: (!list.description.is_empty()).then(|| list.description.clone()),
            images: cover.into_iter().collect(),
            owner: Owner {
                id: Some(ACCOUNT_ID.into()),
                display_name: self.user().display_name,
                uri: None,
            },
            public: Some(false),
            // Smart playlists fill themselves; songs can't be added by hand.
            collaborative: false,
            snapshot_id: Some(tag_id("snapshot", &[&list.tracks.join(",")])),
            tracks: Some(TrackCount {
                total: list.tracks.len() as u32,
            }),
            ..Playlist::default()
        }
    }

    // ----- Answers -----

    pub fn playlists(&self, offset: u32, limit: u32) -> Page<Playlist> {
        let lists: Vec<&List> = self.lists.iter().filter(|list| !list.folder).collect();
        page(&lists, offset, limit, |list| self.playlist_model(list))
    }

    pub fn playlist(&self, id: &str) -> Option<Playlist> {
        self.list(id).map(|list| self.playlist_model(list))
    }

    pub fn editable_playlists(&self) -> impl Iterator<Item = String> + '_ {
        self.lists
            .iter()
            .filter(|list| !list.smart && !list.folder)
            .map(|list| playlist_uri(&list.id))
    }

    pub fn playlist_items(&self, id: &str, offset: u32, limit: u32) -> Option<Page<PlaylistItem>> {
        let list = self.list(id)?;
        let songs: Vec<&Song> = list.tracks.iter().filter_map(|id| self.song(id)).collect();
        Some(page(&songs, offset, limit, |song| PlaylistItem {
            added_at: song.date_added.clone(),
            item: Some(PlayableItem::Track(self.track(song))),
            ..PlaylistItem::default()
        }))
    }

    /// The playlist tree, folders included, in Music.app's order.
    pub fn rootlist(&self) -> Vec<RootlistEntry> {
        let mut entries = Vec::new();
        self.push_children(None, &mut entries);
        entries
    }

    fn push_children(&self, parent: Option<&str>, entries: &mut Vec<RootlistEntry>) {
        for list in self
            .lists
            .iter()
            .filter(|list| list.parent.as_deref() == parent)
        {
            if list.folder {
                entries.push(RootlistEntry::FolderStart {
                    id: list.id.clone(),
                    name: list.name.clone(),
                });
                self.push_children(Some(&list.id), entries);
                entries.push(RootlistEntry::FolderEnd);
            } else {
                entries.push(RootlistEntry::Playlist(playlist_uri(&list.id)));
            }
        }
    }

    /// Favourited songs, newest first.
    fn liked(&self) -> Vec<&Song> {
        let mut liked: Vec<&Song> = self.songs.iter().filter(|song| song.favorited).collect();
        liked.sort_by(|a, b| b.date_added.cmp(&a.date_added));
        liked
    }

    pub fn liked_ids(&self) -> Vec<String> {
        self.liked()
            .into_iter()
            .map(|song| song.id.clone())
            .collect()
    }

    pub fn saved_tracks(&self, offset: u32, limit: u32) -> Page<SavedTrack> {
        page(&self.liked(), offset, limit, |song| SavedTrack {
            added_at: song.date_added.clone(),
            track: self.track(song),
        })
    }

    pub fn saved_albums(&self, offset: u32, limit: u32) -> Page<SavedAlbum> {
        let mut albums: Vec<&AlbumEntry> = self.albums.iter().collect();
        albums.sort_by(|a, b| b.newest_added.cmp(&a.newest_added));
        page(&albums, offset, limit, |album| SavedAlbum {
            added_at: album.newest_added.clone(),
            album: self.album_model(album),
        })
    }

    /// Artists with an album in the library, by name. The cursor is the
    /// position to continue from.
    pub fn followed_artists(&self, after: Option<&str>, limit: u32) -> CursorPage<Artist> {
        let mut artists: Vec<&ArtistEntry> = self
            .artists
            .iter()
            .filter(|artist| !artist.albums.is_empty())
            .collect();
        artists.sort_by_key(|artist| artist.name.to_lowercase());
        let start = after.and_then(|after| after.parse().ok()).unwrap_or(0usize);
        let end = (start + limit as usize).min(artists.len());
        let more = end < artists.len();
        CursorPage {
            items: artists[start.min(end)..end]
                .iter()
                .map(|artist| self.artist_model(artist))
                .collect(),
            total: Some(artists.len() as u32),
            next: more.then(|| format!("after={end}")),
            cursors: Some(Cursors {
                after: more.then(|| end.to_string()),
                before: None,
            }),
        }
    }

    pub fn album(&self, id: &str) -> Option<Album> {
        let album = &self.albums[*self.album_index.get(id)?];
        let mut model = self.album_model(album);
        model.tracks = Some(self.album_tracks_entry(album, 0, u32::MAX));
        Some(model)
    }

    fn album_tracks_entry(&self, album: &AlbumEntry, offset: u32, limit: u32) -> Page<Track> {
        let songs: Vec<&Song> = album
            .songs
            .iter()
            .map(|&index| &self.songs[index])
            .collect();
        page(&songs, offset, limit, |song| self.track(song))
    }

    pub fn album_tracks(&self, id: &str, offset: u32, limit: u32) -> Option<Page<Track>> {
        let album = &self.albums[*self.album_index.get(id)?];
        Some(self.album_tracks_entry(album, offset, limit))
    }

    pub fn album_track_ids(&self, id: &str) -> Option<Vec<String>> {
        let album = &self.albums[*self.album_index.get(id)?];
        Some(
            album
                .songs
                .iter()
                .map(|&index| self.songs[index].id.clone())
                .collect(),
        )
    }

    fn artist_entry(&self, id: &str) -> Option<&ArtistEntry> {
        self.artist_index.get(id).map(|&slot| &self.artists[slot])
    }

    pub fn artist(&self, id: &str) -> Option<Artist> {
        self.artist_entry(id)
            .map(|artist| self.artist_model(artist))
    }

    /// The artist's most played songs.
    pub fn artist_top_tracks(&self, id: &str) -> Option<Vec<Track>> {
        let artist = self.artist_entry(id)?;
        let mut songs: Vec<&Song> = artist
            .songs
            .iter()
            .map(|&index| &self.songs[index])
            .collect();
        songs.sort_by(|a, b| {
            b.played_count
                .cmp(&a.played_count)
                .then(a.name.cmp(&b.name))
        });
        songs.dedup_by(|a, b| a.id == b.id);
        Some(
            songs
                .into_iter()
                .take(10)
                .map(|song| self.track(song))
                .collect(),
        )
    }

    pub fn artist_track_ids(&self, id: &str) -> Option<Vec<String>> {
        let artist = self.artist_entry(id)?;
        let mut ids: Vec<String> = artist
            .albums
            .iter()
            .flat_map(|&slot| self.albums[slot].songs.iter())
            .chain(artist.songs.iter())
            .map(|&index| self.songs[index].id.clone())
            .collect();
        let mut seen = std::collections::HashSet::new();
        ids.retain(|id| seen.insert(id.clone()));
        Some(ids)
    }

    pub fn artist_albums(&self, id: &str, offset: u32, limit: u32) -> Option<Page<Album>> {
        let artist = self.artist_entry(id)?;
        let albums: Vec<&AlbumEntry> = artist
            .albums
            .iter()
            .map(|&slot| &self.albums[slot])
            .collect();
        Some(page(&albums, offset, limit, |album| {
            self.album_model(album)
        }))
    }

    /// Other artists sharing this one's genres, most played first.
    pub fn related_artists(&self, id: &str) -> Option<Vec<Artist>> {
        let artist = self.artist_entry(id)?;
        let genres = self.artist_model(artist).genres;
        let mut related: Vec<&ArtistEntry> = self
            .artists
            .iter()
            .filter(|other| other.id != artist.id && !other.albums.is_empty())
            .filter(|other| {
                other
                    .songs
                    .iter()
                    .any(|&index| genres.contains(&self.songs[index].genre))
            })
            .collect();
        related.sort_by_key(|artist| std::cmp::Reverse(artist.played));
        Some(
            related
                .into_iter()
                .take(20)
                .map(|artist| self.artist_model(artist))
                .collect(),
        )
    }

    /// Most played songs.
    pub fn top_tracks(&self, offset: u32, limit: u32) -> Page<Track> {
        let mut songs: Vec<&Song> = self
            .songs
            .iter()
            .filter(|song| song.played_count > 0)
            .collect();
        songs.sort_by_key(|song| std::cmp::Reverse(song.played_count));
        page(&songs, offset, limit, |song| self.track(song))
    }

    pub fn top_artists(&self) -> Vec<Artist> {
        let mut artists: Vec<&ArtistEntry> = self
            .artists
            .iter()
            .filter(|artist| artist.played > 0)
            .collect();
        artists.sort_by_key(|artist| std::cmp::Reverse(artist.played));
        artists
            .into_iter()
            .take(20)
            .map(|artist| self.artist_model(artist))
            .collect()
    }

    /// Songs by the most played artists that have hardly been played: a
    /// library's own "rediscover".
    pub fn recommendations(&self) -> Vec<Track> {
        let favourites: Vec<&ArtistEntry> = {
            let mut artists: Vec<&ArtistEntry> = self.artists.iter().collect();
            artists.sort_by_key(|artist| std::cmp::Reverse(artist.played));
            artists.into_iter().take(10).collect()
        };
        let mut songs: Vec<&Song> = favourites
            .iter()
            .flat_map(|artist| artist.songs.iter().map(|&index| &self.songs[index]))
            .filter(|song| song.played_count <= 1)
            .collect();
        songs.sort_by(|a, b| a.played_date.cmp(&b.played_date).then(a.name.cmp(&b.name)));
        songs.dedup_by(|a, b| a.id == b.id);
        songs
            .into_iter()
            .take(20)
            .map(|song| self.track(song))
            .collect()
    }

    /// Songs by when they were last played, newest first.
    pub fn recently_played(&self, limit: u32) -> CursorPage<PlayHistory> {
        let mut songs: Vec<&Song> = self
            .songs
            .iter()
            .filter(|song| song.played_date.is_some())
            .collect();
        songs.sort_by(|a, b| b.played_date.cmp(&a.played_date));
        CursorPage {
            items: songs
                .into_iter()
                .take(limit as usize)
                .map(|song| PlayHistory {
                    track: self.track(song),
                    played_at: song.played_date.clone(),
                    context: None,
                })
                .collect(),
            total: None,
            next: None,
            cursors: None,
        }
    }

    /// Songs, albums, artists and playlists whose names match every word of
    /// the query, best matches first.
    pub fn search(&self, query: &str, limit: u32) -> SearchResults {
        let words: Vec<String> = query
            .split_whitespace()
            .map(|word| word.to_lowercase())
            .collect();
        if words.is_empty() {
            return SearchResults::default();
        }
        let score = |name: &str, extra: &[&str]| -> Option<u32> {
            let name = name.to_lowercase();
            let haystack = std::iter::once(name.as_str())
                .chain(extra.iter().copied())
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase();
            if !words.iter().all(|word| haystack.contains(word.as_str())) {
                return None;
            }
            let whole = words.join(" ");
            Some(if name == whole {
                0
            } else if name.starts_with(&whole) {
                1
            } else if words.iter().all(|word| name.contains(word.as_str())) {
                2
            } else {
                3
            })
        };
        fn ranked<T>(mut found: Vec<(u32, &T)>, limit: u32) -> Vec<&T> {
            found.sort_by_key(|(rank, _)| *rank);
            found
                .into_iter()
                .take(limit as usize)
                .map(|(_, item)| item)
                .collect()
        }
        let tracks = ranked(
            self.songs
                .iter()
                .filter_map(|song| {
                    score(&song.name, &[&song.artist, &song.album]).map(|rank| (rank, song))
                })
                .collect(),
            limit,
        );
        let albums = ranked(
            self.albums
                .iter()
                .filter_map(|album| score(&album.name, &[&album.artist]).map(|rank| (rank, album)))
                .collect(),
            limit,
        );
        let artists = ranked(
            self.artists
                .iter()
                .filter_map(|artist| score(&artist.name, &[]).map(|rank| (rank, artist)))
                .collect(),
            limit,
        );
        let lists = ranked(
            self.lists
                .iter()
                .filter(|list| !list.folder)
                .filter_map(|list| score(&list.name, &[]).map(|rank| (rank, list)))
                .collect(),
            limit,
        );
        SearchResults {
            tracks: Some(page(&tracks, 0, limit, |song| self.track(song))),
            albums: Some(page(&albums, 0, limit, |album| self.album_model(album))),
            artists: Some(page(&artists, 0, limit, |artist| self.artist_model(artist))),
            playlists: Some(page(&lists, 0, limit, |list| self.playlist_model(list))),
            shows: None,
            episodes: None,
        }
    }

    pub fn contains(&self, uris: &[String]) -> Vec<bool> {
        uris.iter()
            .map(|uri| {
                crate::util::uri_id(uri)
                    .and_then(|id| self.song(id))
                    .is_some_and(|song| song.favorited)
            })
            .collect()
    }
}

fn image(track_id: &str) -> Image {
    Image {
        url: art_url(track_id),
        width: Some(600),
        height: Some(600),
    }
}

fn page<S, T>(items: &[S], offset: u32, limit: u32, convert: impl Fn(&S) -> T) -> Page<T> {
    let total = items.len() as u32;
    let start = offset.min(total) as usize;
    let end = offset.saturating_add(limit).min(total) as usize;
    Page {
        items: items[start..end].iter().map(convert).collect(),
        total,
        limit,
        offset,
        next: (end < items.len()).then(|| format!("offset={end}")),
    }
}

/// Groups `ids` that are songs and that are not, keeping the library's
/// own songs only. Used where the interface hands over URIs.
pub fn track_ids(uris: &[String]) -> Vec<String> {
    uris.iter()
        .filter(|uri| crate::util::uri_kind(uri) == Some("track"))
        .filter_map(|uri| crate::util::uri_id(uri).map(str::to_string))
        .collect()
}

/// How many songs each album has, for tests and diagnostics.
#[cfg(test)]
fn album_sizes(library: &Library) -> BTreeMap<String, usize> {
    library
        .albums
        .iter()
        .map(|album| (album.name.clone(), album.songs.len()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(id: &str, name: &str, artist: &str, album: &str, track: u32, plays: u32) -> Song {
        Song {
            id: id.into(),
            name: name.into(),
            artist: artist.into(),
            album: album.into(),
            album_artist: String::new(),
            duration_ms: 180_000,
            track_number: track,
            disc_number: 1,
            year: 2002,
            genre: "Industrial".into(),
            favorited: false,
            played_count: plays,
            played_date: None,
            date_added: Some(format!("2026-01-0{track}T00:00:00.000Z")),
        }
    }

    fn library() -> Library {
        let mut songs = vec![
            song(
                "A1",
                "Parasite Mine",
                "Acumen Nation",
                "The Fifth Column",
                2,
                13,
            ),
            song("A2", "Gun Lap", "Acumen Nation", "The Fifth Column", 1, 0),
            song(
                "B1",
                "Closer",
                "Nine Inch Nails",
                "The Downward Spiral",
                1,
                40,
            ),
        ];
        songs[2].favorited = true;
        let lists = vec![
            List {
                id: "F1".into(),
                name: "Folder".into(),
                description: String::new(),
                smart: false,
                folder: true,
                parent: None,
                tracks: vec![],
            },
            List {
                id: "P1".into(),
                name: "ankara".into(),
                description: String::new(),
                smart: false,
                folder: false,
                parent: Some("F1".into()),
                tracks: vec!["B1".into(), "A1".into(), "gone".into()],
            },
        ];
        Library::new(songs, lists)
    }

    #[test]
    fn albums_are_grouped_and_ordered_by_track() {
        let library = library();
        assert_eq!(
            album_sizes(&library),
            BTreeMap::from([
                ("The Downward Spiral".to_string(), 1),
                ("The Fifth Column".to_string(), 2),
            ])
        );
        let id = library.track(library.song("A1").unwrap()).album.unwrap().id;
        let names: Vec<String> = library
            .album(&id)
            .unwrap()
            .tracks
            .unwrap()
            .items
            .into_iter()
            .map(|track| track.name)
            .collect();
        assert_eq!(names, ["Gun Lap", "Parasite Mine"]);
    }

    #[test]
    fn ids_round_trip_through_uris() {
        let library = library();
        let track = library.track(library.song("A1").unwrap());
        assert_eq!(track.uri, "spotify:track:A1");
        let artist = &track.artists[0];
        let id = artist.id.as_deref().unwrap();
        assert_eq!(
            artist.uri.as_deref(),
            Some(format!("spotify:artist:{id}").as_str())
        );
        assert_eq!(library.artist(id).unwrap().name, "Acumen Nation");
        assert_eq!(
            art_track(&track.album.unwrap().images[0].url),
            Some("A2"),
            "an album's cover is its first song's"
        );
    }

    #[test]
    fn playlists_skip_folders_and_missing_songs() {
        let library = library();
        let lists = library.playlists(0, 50);
        assert_eq!(lists.items.len(), 1);
        assert_eq!(lists.items[0].uri, "spotify:playlist:P1");
        let items = library.playlist_items("P1", 0, 50).unwrap();
        assert_eq!(
            items.total, 2,
            "a song no longer in the library is left out"
        );
    }

    #[test]
    fn rootlist_keeps_folders() {
        assert_eq!(
            library().rootlist(),
            vec![
                RootlistEntry::FolderStart {
                    id: "F1".into(),
                    name: "Folder".into()
                },
                RootlistEntry::Playlist("spotify:playlist:P1".into()),
                RootlistEntry::FolderEnd,
            ]
        );
    }

    #[test]
    fn liked_songs_are_the_favourites() {
        let mut library = library();
        assert_eq!(library.saved_tracks(0, 50).total, 1);
        library.set_favorited("A1", true);
        assert_eq!(library.liked_ids(), ["A1", "B1"], "newest added first");
        assert_eq!(
            library.contains(&["spotify:track:A1".into(), "spotify:track:A2".into()]),
            [true, false]
        );
    }

    #[test]
    fn pages_report_what_follows() {
        let library = library();
        let first = library.top_tracks(0, 1);
        assert_eq!(first.items[0].name, "Closer");
        assert_eq!(first.total, 2, "unplayed songs are not top tracks");
        assert!(first.next.is_some());
        assert!(library.top_tracks(1, 1).next.is_none());
    }

    #[test]
    fn search_matches_every_word_and_ranks_names() {
        let library = library();
        let results = library.search("acumen parasite", 20);
        let tracks = results.tracks.unwrap();
        assert_eq!(tracks.items.len(), 1);
        assert_eq!(tracks.items[0].name, "Parasite Mine");
        let results = library.search("closer", 20);
        assert_eq!(results.tracks.unwrap().items[0].name, "Closer");
        assert!(library.search("  ", 20).is_empty());
    }

    #[test]
    fn snapshot_parses_the_script_output() {
        let snapshot: Snapshot = serde_json::from_str(
            r#"{"tracks":{"id":["A"],"name":["n"],"artist":["a"],"album":["b"],
                "album_artist":[""],"duration":[1.5],"track_number":[1],"disc_number":[1],
                "year":[2000],"genre":["g"],"favorited":[true],"played_count":[2],
                "played_date":[null],"date_added":["2026-01-01T00:00:00.000Z"]},
               "playlists":{"id":["L","M","Q"],"name":["Music","mine","Spotifast Queue"],
                "description":["","",""],"smart":[false,false,false],
                "special":["Music","none","none"],"class":["userPlaylist","userPlaylist","userPlaylist"],
                "parent":[null,null,null],"tracks":[["A"],["A"],["A"]]}}"#,
        )
        .expect("snapshot");
        let library = Library::from_snapshot(snapshot);
        assert_eq!(library.song("A").unwrap().duration_ms, 1500);
        assert_eq!(
            library.playlists(0, 50).items.len(),
            1,
            "the library view and the queue playlist are not listed"
        );
    }
}
