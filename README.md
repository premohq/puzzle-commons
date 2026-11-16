# Puzzle Commons

The open-source puzzle platform: four puzzle sports (Quad, Chain, Capsule, Panel) in one match, up to six seats, local versus with character select, bots, a story told across five 3D worlds by an anime cast, a training curriculum taught by Mira, replays. Custom Rust engine: our own deterministic sim, software renderer, synth, and input stack. License: GPL-3.0 (see `LICENSE`).

## Download

Ready-made builds come from GitHub Actions (`.github/workflows/build.yml`). Every push builds all three; pushing a tag such as `v1.0.0` also publishes them on the Releases page.

- **Windows:** unzip `puzzle-commons-windows-x86_64.zip` and run `Puzzle Commons.exe`. SmartScreen may warn because the build is unsigned: choose *More info → Run anyway*.
- **macOS (Apple silicon and Intel, macOS 11+):** unzip `puzzle-commons-macos-universal.zip` and open `Puzzle Commons.app`. The app is not notarized, so the first time right-click it and choose *Open*, or run `xattr -dr com.apple.quarantine "Puzzle Commons.app"`.
- **Linux (x86_64, X11 or XWayland):** `tar xzf puzzle-commons-linux-x86_64.tar.gz` and run `./puzzle-commons`.

## Build it yourself

Install Rust once (https://rustup.rs), then from this folder:

- **Linux / macOS:** `cargo run --release -p purtris-client`
- **Windows:** `cargo run --release -p purtris-client` (in PowerShell or cmd)

The built game is `target/release/puzzle-commons` (`.exe` on Windows). It embeds all art, so that single file can be copied anywhere.

Controls: the arrow keys alone play every style (left/right move, up rotates, down drops faster); Space hard drops, Z/X rotate, C holds, Esc pauses. Any gamepad works (auto-detected via the SDL controller database), and the mouse works in menus. Two or three people can share one keyboard; every key can be rebound in Settings.

On first launch Mira walks you through setup (picture, controls, sound, your character) and offers a tour of the modes; the tour is always in the Settings menu.

## Display

Settings → Display: fullscreen or windowed (720p to 4K window sizes), render resolution (native, or a fixed size scaled smoothly or sharply), and a frame-rate limit that matches your monitor's refresh rate by default. The renderer is resolution independent and multi-threaded: every screen draws in under 6 ms at 2560x1440 on a 4-core laptop CPU (`puzzle-commons --bench` prints the timings).

## Tests and tools

- `cargo run --release -p purtris-tools` runs determinism, replay, puzzle-solvability and bot soak checks. CI runs `determinism`, `puzzles` and `bench` before every build.
- `cargo check -p purtris-client --features check_all` type-checks the Windows and macOS audio back ends from any OS.
- `puzzle-commons --shots DIR [WIDTH]` saves screenshots of every screen; `--bench` times frames at 720p, 1080p and 1440p.
- Live tests: `PC_SCRIPT=file PC_WINDOWED=1 XDG_CONFIG_HOME=/some/scratch/dir puzzle-commons` plays a script of timed key presses and screenshots (`<seconds> key Enter`, `hold Left 0.5`, `shot out.png`, `quit`) in the real window without touching your own settings.

## Rebuilding the art (optional, needs Blender 5)

`python3 tools/fetch_assets.py` downloads the CC0 Poly Haven sources; CC0 model packs go in `third_party/oga/`. Then run the scripts in `tools/blender/` (`overworld.py`, `export_cast.py`, `font_atlas.py`, `icons.py`, `logo.py`) with `blender -b --factory-startup --python <script> -- <args>`.

The cast: put the VRoid CC0 sample avatars (see `third_party/vroid_cc0/LICENSE.txt` for the source) as `.vrm` files in `third_party/vroid_cc0/vrm/`, then `blender -b --factory-startup --python tools/blender/cast3d.py -- <repo> cast` renders every character in eight expressions (full body and portrait) into `assets/cast/`, which the build embeds.

## Credits (all free licenses)

- Fonts: Lato, M PLUS Rounded 1c, Mochiy Pop One (SIL OFL 1.1, `assets/fonts/`)
- Icons and tile symbols: Phosphor Icons (MIT)
- Controller mappings: SDL_GameControllerDB (zlib, `assets/input/`)
- Characters (Mira, Kai, the five rivals and the arcade regulars): the VRoid Studio sample avatars by the VRoid Project / pixiv Inc., released under CC0 (`third_party/vroid_cc0/LICENSE.txt`), posed, recolored and toon-rendered for this game.
- 3D sources: Quaternius Stylized Nature MegaKit and Medieval Village MegaKit, Kenney Nature Kit, Poly Haven (all CC0)
