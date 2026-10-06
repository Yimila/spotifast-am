// What Music.app is doing right now, as JSON. Never launches Music.app:
// `running()` is answered by the system, not by the app.
function run() {
  const music = Application("Music");
  if (!music.running()) return JSON.stringify({ running: false });
  let track = null;
  try {
    const current = music.currentTrack;
    track = {
      id: current.persistentID(),
      name: current.name(),
      artist: current.artist(),
      album: current.album(),
      duration: current.duration(),
    };
  } catch (error) {
    track = null;
  }
  let playlist = null;
  try {
    playlist = music.currentPlaylist.persistentID();
  } catch (error) {
    playlist = null;
  }
  return JSON.stringify({
    running: true,
    state: music.playerState(),
    position: music.playerPosition() || 0,
    volume: music.soundVolume(),
    shuffle: music.shuffleEnabled(),
    repeat: music.songRepeat(),
    track,
    playlist,
  });
}
