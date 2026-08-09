# Slingshot

A two-player gravity slingshot game, playable in the browser or as a native binary with optional network multiplayer.

Inspired by the classic [Slingshot](https://github.com/ryanakca/slingshot) (Jonathan Musther & Bart Mak, 2007).

**[Play in browser →](https://barafael.github.io/gnils/)**

## Gameplay

Two players take turns firing missiles across a field of planets. Shots follow curved paths under each planet's gravity. Hit your opponent to score; miss and the turn passes. Whoever leads when the round limit is reached wins.

| Key | Action |
| --- | --- |
| `↑` / `↓` | Increase / decrease power |
| `←` / `→` | Adjust angle |
| `Space` / `Enter` | Fire |
| Hold `Shift` | 5× coarser adjustment |
| Hold `Ctrl` | Fine adjustment |
| Hold `Alt` | Ultra-fine adjustment |
| `Escape` | Open/close settings menu |

**Scoring:** +1500 for hitting the opponent (minus a penalty for slow shots), −2000 for self-hits.

## Modes

### Local hotseat

Both players share the same keyboard. Start from the main menu → **New Game**.

### Network multiplayer

Peer-to-peer over WebRTC, relayed by a public signaling server. No server to run, no certificates to exchange. Both players share a room: one hosts, the other joins.

**Hosting:**

1. Main menu → **Network → Host**
2. A room id is generated (shown in the address bar on the web build)
3. Share the room id with your opponent

**Joining:**

1. Main menu → **Network → Join**
2. Enter the host's room id
3. Press `Enter` to connect; the game starts automatically when the second player is in the room

> Works identically in the browser and native — anyone can host or join.

## Running locally

Requires [Rust](https://rustup.rs/).

```sh
cargo run                      # native client
```

For the browser build, install [trunk](https://trunkrs.dev/) and run `trunk serve`.

## Settings

Accessible via `Escape` during a game:

| Setting | Default | Description |
| --- | --- | --- |
| Max planets | 4 | Planets per round (2–4) |
| Max blackholes | 0 | Blackholes that absorb shots (0–3) |
| Bounce | Off | Shots bounce off screen edges |
| Invisible planets | Off | Planets hidden after round setup |
| Fixed power | Off | Power locked; angle-only aiming |
| Particles | On | Explosion particle effects |
| Max rounds | ∞ | 0 = unlimited, or 5 / 10 / 20 |
| Fullscreen | Off | Borderless fullscreen |

## Credits

Original Slingshot by Jonathan Musther & Bart Mak (2007), later maintained by Ryan Kavanagh. This is a full rewrite in Rust/Bevy, preserving the original physics and gameplay.

## License

GPL-2.0, following the original Slingshot license.
