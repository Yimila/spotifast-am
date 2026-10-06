// Every change Spotifast asks of Music.app, one action per run. The first
// argument names the action; the rest are its arguments. Prints JSON.
const QUEUE_NAME = "Spotifast Queue";

function run(argv) {
  const music = Application("Music");
  const action = argv[0];
  const args = argv.slice(1);
  const library = () => music.libraryPlaylists[0];
  const libraryTrack = (id) => {
    const found = library().tracks.whose({ persistentID: id });
    if (found.length === 0) throw new Error("track not found: " + id);
    return found[0];
  };
  const playlist = (id) => {
    const found = music.playlists.whose({ persistentID: id });
    if (found.length === 0) throw new Error("playlist not found: " + id);
    return found[0];
  };
  const queue = () => {
    const found = music.userPlaylists.whose({ name: QUEUE_NAME });
    if (found.length > 0) return found[0];
    return music.make({ new: "userPlaylist", withProperties: { name: QUEUE_NAME } });
  };
  const playingFromQueue = () => {
    try {
      return music.currentPlaylist.name() === QUEUE_NAME;
    } catch (error) {
      return false;
    }
  };

  switch (action) {
    case "toggle":
      music.playpause();
      break;
    case "play":
      music.play();
      break;
    case "pause":
      music.pause();
      break;
    case "next":
      music.nextTrack();
      break;
    case "previous":
      music.backTrack();
      break;
    case "seek":
      music.playerPosition = Number(args[0]) / 1000;
      break;
    case "volume":
      music.soundVolume = Number(args[0]);
      break;
    case "shuffle":
      music.shuffleEnabled = args[0] === "true";
      break;
    case "repeat":
      music.songRepeat = args[0];
      break;
    case "play_playlist": {
      const list = playlist(args[0]);
      if (args[1]) {
        const found = list.tracks.whose({ persistentID: args[1] });
        music.play(found.length ? found[0] : list);
      } else {
        music.play(list);
      }
      break;
    }
    case "play_list": {
      // Music.app has no album context and no scriptable Up Next, so an
      // album, an artist, or a list of songs plays from one playlist that
      // Spotifast owns. The first song starts as soon as it is added.
      const list = queue();
      list.tracks().forEach((track) => track.delete());
      let started = false;
      for (const id of args) {
        try {
          const copy = music.duplicate(libraryTrack(id), { to: list });
          if (!started) {
            music.play(copy);
            started = true;
          }
        } catch (error) {
          // A song that left the library is skipped, not fatal.
        }
      }
      if (!started) throw new Error("none of these songs are in the library");
      break;
    }
    case "enqueue": {
      if (!playingFromQueue()) return JSON.stringify({ ok: false, reason: "not_queue" });
      const list = queue();
      for (const id of args) music.duplicate(libraryTrack(id), { to: list });
      return JSON.stringify({ ok: true, added: args.length });
    }
    case "upcoming": {
      // The songs after the current one, when playing from the queue playlist.
      if (!playingFromQueue()) return JSON.stringify({ ok: true, ids: [] });
      const ids = queue().tracks.persistentID();
      let current = -1;
      try {
        current = music.currentTrack.index() - 1;
      } catch (error) {
        current = -1;
      }
      return JSON.stringify({ ok: true, ids: ids.slice(current + 1) });
    }
    case "favorite": {
      const value = args[0] === "true";
      for (const id of args.slice(1)) libraryTrack(id).favorited = value;
      break;
    }
    case "create_playlist": {
      const made = music.make({
        new: "userPlaylist",
        withProperties: { name: args[0], description: args[1] || "" },
      });
      return JSON.stringify({ ok: true, id: made.persistentID() });
    }
    case "rename_playlist":
      playlist(args[0]).name = args[1];
      break;
    case "describe_playlist":
      playlist(args[0]).description = args[1];
      break;
    case "add_to_playlist": {
      const list = playlist(args[0]);
      for (const id of args.slice(1)) music.duplicate(libraryTrack(id), { to: list });
      break;
    }
    case "remove_from_playlist": {
      const list = playlist(args[0]);
      const remove = new Set(args.slice(1));
      const tracks = list.tracks();
      // Back to front, so deleting does not shift the rows still to visit.
      for (let index = tracks.length - 1; index >= 0; index--) {
        if (remove.has(tracks[index].persistentID())) tracks[index].delete();
      }
      break;
    }
    default:
      throw new Error("unknown action: " + action);
  }
  return JSON.stringify({ ok: true });
}
