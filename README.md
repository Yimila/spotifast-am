# Spotifast

> **This fork plays Apple Music on macOS.** On a Mac, Spotifast drives
> Music.app instead of signing in to Spotify: your library, playlists
> (folders included), favourites, play counts and artwork come from
> Music.app, and playback runs there. Apple Music audio is FairPlay
> protected, so no third-party app can stream it itself.
>
> - No sign-in. The first launch asks to control Music
>   (System Settings → Privacy & Security → Automation).
> - Albums, artists, Liked Songs and song lists play from a playlist named
>   **Spotifast Queue**, which Spotifast creates and refills. Adding to the
>   queue works while that playlist is playing. Music.app's own Up Next
>   can't be scripted.
> - Home is built from your library: recently played, most played songs
>   and artists, and songs by your top artists you've hardly played.
> - Not available: catalogue search beyond your library, radio, podcasts,
>   playlist covers and reordering, the visualizer and EQ (the audio never
>   passes through Spotifast).
> - `SPOTIFAST_BACKEND=spotify` runs the original Spotify backend. Linux
>   and Windows builds are unchanged.
>
> The code lives in `src/apple_music/`. Catalogue search through the
> Apple Music API would need an Apple Developer account and is not built
> yet.

**Spotify, native and fast.** Spotifast is a Spotify client written in
Rust with [egui](https://github.com/emilk/egui). It plays music through
[librespot](https://github.com/librespot-org/librespot), typically uses
100–250 MB of RAM, starts in well under a second, and has no browser engine.
It runs on Linux, macOS, and Windows.

**Playback needs Spotify Premium.** Free accounts can browse and search, but
cannot play music through Spotifast.

https://github.com/user-attachments/assets/a5f669ce-b3b7-4f8e-9933-976a78876c7e

![Spotifast Home with the playlist library, recommendations, queue, and player visible](docs/screenshot.png)

**[spotifast.rocks](https://spotifast.rocks/)** has downloads and the full guide:

- [Getting started](https://spotifast.rocks/getting-started/): sign-in, playback on this computer, themes, fonts, proxies
- [Everyday use](https://spotifast.rocks/using-spotifast/): keyboard shortcuts, command-line control, updates
- [Settings and files](https://spotifast.rocks/settings-and-files/) and [Privacy](https://spotifast.rocks/privacy/)
- [How it connects](https://spotifast.rocks/how-it-connects/) and [What Spotify allows](https://spotifast.rocks/what-spotify-allows/)
- [Will my account get banned?](https://spotifast.rocks/what-is-spotifast/#will-my-spotify-account-get-banned)

**Want WhatsApp just as fast and native?** [ZapFast](https://zapfast.rocks)
is Spotifast's sibling. Both are built on
[fastframe](https://github.com/crmne/fastframe).

## Install

- **macOS:** `brew install --cask crmne/tap/spotifast`, or
  [download the Mac app](https://spotifast.rocks/download/#macos).
- **Arch Linux:** `yay -S spotifast-bin`
- **Windows, Flatpak, AppImage, Nix and other Linux:** see the
  [Download page](https://spotifast.rocks/download/).
- **From source:** see
  [Build from source](https://spotifast.rocks/getting-started/#build-from-source).

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening an issue or pull
request. To look at the interface without a Spotify account, run
`cargo run --features demo -- --demo`. Translations live in `assets/i18n/`;
see [Translating Spotifast](docs/_reference/translating.md). Release
packaging is described in [PACKAGING.md](PACKAGING.md).

## Acknowledgements

Spotifast uses [librespot](https://github.com/librespot-org/librespot),
[egui](https://github.com/emilk/egui), the [Inter](https://rsms.me/inter/)
typeface (OFL), and [Lucide](https://lucide.dev) icons (ISC).

Spotifast is an independent project and is not affiliated with Spotify.
Spotify is a trademark of Spotify AB.

Licensed under the [MIT License](LICENSE).
