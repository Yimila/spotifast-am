// Reads the whole Music.app library in a handful of bulk Apple Event
// requests and prints it as JSON. Reading a property of a whole element
// collection is one round trip; reading it per track would be hundreds.
function run() {
  const music = Application("Music");
  const library = music.libraryPlaylists[0];
  const tracks = library.tracks;
  const count = tracks.length;
  const read = (property) => (count ? tracks[property]() : []);
  const dates = (values) => values.map((value) => (value ? value.toISOString() : null));

  const playlists = music.userPlaylists;
  const playlistCount = playlists.length;
  const playlistRead = (property) => (playlistCount ? playlists[property]() : []);
  const playlistIds = playlistRead("persistentID");
  const parents = [];
  const members = [];
  for (let index = 0; index < playlistCount; index++) {
    const playlist = playlists[index];
    let parent = null;
    try {
      parent = playlist.parent.persistentID();
    } catch (error) {
      parent = null;
    }
    parents.push(parent);
    let ids = [];
    try {
      ids = playlist.tracks.persistentID();
    } catch (error) {
      ids = [];
    }
    members.push(ids);
  }

  return JSON.stringify({
    tracks: {
      id: read("persistentID"),
      name: read("name"),
      artist: read("artist"),
      album: read("album"),
      album_artist: read("albumArtist"),
      duration: read("duration"),
      track_number: read("trackNumber"),
      disc_number: read("discNumber"),
      year: read("year"),
      genre: read("genre"),
      favorited: read("favorited"),
      played_count: read("playedCount"),
      played_date: dates(read("playedDate")),
      date_added: dates(read("dateAdded")),
    },
    playlists: {
      id: playlistIds,
      name: playlistRead("name"),
      description: playlistRead("description"),
      smart: playlistRead("smart"),
      special: playlistRead("specialKind"),
      class: playlistRead("class"),
      parent: parents,
      tracks: members,
    },
  });
}
