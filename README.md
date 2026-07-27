<div align="center">

# Flodo

**A floating to-do list that stays out of your way.**

One infinite list, a checkbox, and a toggle to hide what you've finished.
That's the whole app.

[![CI](https://github.com/michellemayes/flodo/actions/workflows/ci.yml/badge.svg)](https://github.com/michellemayes/flodo/actions/workflows/ci.yml)
[![Release](https://github.com/michellemayes/flodo/actions/workflows/release.yml/badge.svg)](https://github.com/michellemayes/flodo/actions/workflows/release.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Rust](https://img.shields.io/badge/rust-stable-orange.svg)
![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey.svg)

<img src="docs/images/hero.png" alt="Flodo showing a short to-do list with one item checked off" width="380">

</div>

---

Flodo is a small frameless panel that floats above your other windows. You type
a to-do, you check it off, and you get on with your day.

It is deliberately **not** a project management tool. There are no tags, no
priorities, no due dates, no projects, and no sub-tasks — and there won't be.

## Why you might want it

| Feature | What that means |
|---|---|
| **It floats** | Frameless and always-on-top, so it sits over whatever you're working in. Drag it by any empty space. Unpin it when it's in the way. |
| **Bodies hold real markdown** | A to-do is one line, but it can carry a description underneath — including fenced code snippets — collapsed until you want it. |
| **It's yours to style** | Eight accent colours, light and dark, your font, your code font, your text size, your row spacing, your opacity. |
| **Keyboard first** | Type, Enter, type, Enter. The field keeps focus so you never reach for the mouse. |
| **It's small** | One ~8 MB binary. No Electron, no webview, no background service, no account. |
| **Your data is yours** | Two plain JSON files you can read, edit, grep, and sync. |

## Install

Download the latest [release](../../releases).

**macOS** — unzip and drag `Flodo.app` to Applications. Builds are ad-hoc signed
but not notarized, so the first launch needs right-click → **Open**, or:

```sh
xattr -dr com.apple.quarantine /Applications/Flodo.app
```

**Linux / Windows** — unpack the archive and run `flodo`.

**From source** — needs a stable Rust toolchain:

```sh
git clone https://github.com/michellemayes/flodo
cd flodo
cargo run --release
```

<details>
<summary>Linux build dependencies</summary>

```sh
sudo apt install libgtk-3-dev libxkbcommon-dev libgl1-mesa-dev \
                 libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
```
</details>

## Using it

Type in the box at the top and press <kbd>Enter</kbd>. The new to-do lands on
the line directly below, and the field keeps focus — so adding five things is
five lines of typing, no clicking.

Click the circle to check something off. It **stays exactly where it is**,
dimmed and struck through, so nothing jumps around under your cursor. When
you've had enough of looking at them, hit the eye to hide completed items.

<div align="center">
<img src="docs/images/light.png" alt="Flodo in light mode with a blue accent" width="340">
</div>

### Markdown, only when you want it

Titles and bodies are markdown. While you're typing you see the raw source;
click away and it renders. No toolbar, no formatting buttons, nothing to learn.

| While you're typing | After you click away |
|---|---|
| <img src="docs/images/editing.png" alt="A to-do being edited, showing raw markdown asterisks" width="330"> | <img src="docs/images/rendered.png" alt="The same to-do rendered, with bold and inline code" width="330"> |

### Bodies

Hover a row and click the chevron to give a to-do a body. That's where the
detail goes — a note, a link, a stack trace, the command you keep forgetting.

<div align="center">
<img src="docs/images/markdown.png" alt="A to-do expanded to show a markdown body with a heading, a link, a Rust code block, nested lists and a blockquote" width="380">
</div>

Bodies support headings, **bold**, *italic*, `inline code`, fenced code blocks
with a language label and a copy button, nested lists, links, blockquotes,
horizontal rules, and ~~strikethrough~~.

Code blocks scroll sideways rather than wrapping, because wrapped code is
unreadable:

````markdown
Races on the session cookie. Reproduce with:

```sh
cargo test --test login -- --test-threads=1
```

- [the flaky run](https://ci.example.com/12345)
- probably the `SameSite` change
````

### Make it yours

<div align="center">
<img src="docs/images/settings.png" alt="The settings sheet showing accent swatches, appearance, font pickers and sliders" width="340">
</div>

Seven settings, one screen, no tabs. Press <kbd>⌘</kbd><kbd>,</kbd>.

Each accent colour tints the *whole* panel — the background, surfaces, and
borders all shift toward its hue at low saturation, so it reads as one designed
thing rather than a coloured button on grey.

| Pink · dark | Green · light | Amber · dark | Purple · light |
|---|---|---|---|
| <img src="docs/images/accent-pink.png" alt="Flodo with a pink accent in dark mode" width="220"> | <img src="docs/images/accent-green.png" alt="Flodo with a green accent in light mode" width="220"> | <img src="docs/images/accent-amber.png" alt="Flodo with an amber accent in dark mode" width="220"> | <img src="docs/images/accent-purple.png" alt="Flodo with a purple accent in light mode" width="220"> |

Every one of the eight accents is contrast-tested in both light and dark — there
is a unit test asserting WCAG AA for body text against the background, so a
palette can't ship unreadable.

## Keyboard

| Shortcut | Action |
|---|---|
| <kbd>Enter</kbd> | Add the to-do, keep focus for the next one |
| <kbd>⌘</kbd><kbd>N</kbd> | Jump to the composer |
| <kbd>⌘</kbd><kbd>E</kbd> | Show / hide completed |
| <kbd>⌘</kbd><kbd>P</kbd> | Pin / unpin from always-on-top |
| <kbd>⌘</kbd><kbd>,</kbd> | Settings |
| <kbd>⌘</kbd><kbd>Z</kbd> | Undo the last delete |
| <kbd>⌘</kbd><kbd>↑</kbd> / <kbd>⌘</kbd><kbd>↓</kbd> | Move the to-do you're editing |
| <kbd>⌘</kbd><kbd>⌫</kbd> | Delete |
| <kbd>Esc</kbd> | Stop editing, or close settings |
| <kbd>⌥</kbd><kbd>Space</kbd> | Summon or hide Flodo from anywhere |

Use <kbd>Ctrl</kbd> instead of <kbd>⌘</kbd> on Linux and Windows. Drag the
handle on the left of a row to reorder it.

## Your data

| Platform | Location |
|---|---|
| macOS | `~/Library/Application Support/Flodo/` |
| Linux | `~/.local/share/flodo/` |
| Windows | `%APPDATA%\Flodo\` |

Two plain JSON files. `todos.json` looks like this:

```json
{
  "version": 1,
  "todos": [
    {
      "id": 7318429184000,
      "title": "Fix the flaky `login_test`",
      "body": "Races on the session cookie.",
      "done": false,
      "created_at": 1785192000,
      "expanded": false
    }
  ]
}
```

Edit it by hand, keep it in a git repo, sync it with Dropbox — it's just a file.
Point `FLODO_STATE_DIR` somewhere else if you'd rather.

Flodo tries hard not to lose it:

- Saves are **atomic** — written to a temp file and renamed into place, so a
  crash mid-write can't leave you with half a list.
- A file that fails to parse is **quarantined**, never overwritten. You get
  `todos.json.corrupt-<timestamp>` with the original bytes intact, and a notice
  in the app.
- Unknown fields **round-trip**, so opening your list in an older build won't
  strip what a newer one wrote.

## Known limitations

Worth knowing before you commit:

- **Emoji render in monochrome.** epaint has no COLR/sbix path, so Apple Color
  Emoji falls back to a bundled monochrome font.
- **Bold and italic need real font faces.** egui has no synthetic emphasis.
  Flodo uses a family's real bold/italic/oblique faces, and can instance a
  variable `wght` or `slnt` axis where one exists (this covers SF Pro and
  Inter). A family with no bold face renders `**bold**` as regular.
- **Text editing is egui's, not the system's** — no spellcheck, no emoji picker,
  no dictation, and only partial IME support.
- **Flodo appears in the Dock and in ⌘-Tab.** A menu-bar-only accessory mode is
  a plausible future change, not a current one.
- **macOS builds are not notarized**, hence the right-click → Open dance.

## Development

```sh
cargo test                                                  # 63 tests
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Tests cover the parts that can be tested without a screen: the model, atomic
writes and corruption handling, settings clamping, the markdown parser
(including a no-panic sweep over pathological input), hotkey parsing, font
validation, and palette contrast.

The GUI is verified by screenshot. `eframe` has a built-in hook that renders a
couple of frames, writes a PNG, and exits — so no display-server tooling is
needed beyond Xvfb:

```sh
xvfb-run -a -s "-screen 0 700x900x24" \
  env LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
      FLODO_STATE_DIR=/tmp/flodo-shots \
      FLODO_DEMO=showcase \
      EFRAME_SCREENSHOT_TO=/tmp/flodo.png \
  cargo run --features screenshot
```

`FLODO_DEMO` seeds a scenario in memory without touching your real list —
`hero`, `showcase`, `editing`, `rendered`, `settings`, `empty`, `long`, `body`.
Every image in this README was produced that way.

> [!NOTE]
> The glow backend is not incidental. eframe's screenshot hook is glow-only, and
> wgpu needs a Vulkan or GLES adapter that headless CI usually lacks. Swapping
> backends would cost this whole verification loop.

### Releasing

Push a semver tag:

```sh
git tag -a v0.1.0 -m "Flodo v0.1.0"
git push origin v0.1.0
```

That builds a universal macOS `.app` (arm64 + x86_64, ad-hoc signed), plus Linux
and Windows archives, and publishes them to a GitHub Release with
`SHA256SUMS.txt`. Tags containing a hyphen — `v0.1.0-rc.1` — publish as
pre-releases.

## Built with

[egui](https://github.com/emilk/egui) ·
[eframe](https://github.com/emilk/egui/tree/master/crates/eframe) ·
[pulldown-cmark](https://github.com/raphlinus/pulldown-cmark) ·
[fontdb](https://github.com/RazrFalcon/fontdb) ·
[skrifa](https://github.com/googlefonts/fontations) ·
[global-hotkey](https://github.com/tauri-apps/global-hotkey) ·
[serde](https://serde.rs)

Seven direct dependencies, on purpose.

## License

[MIT](LICENSE)
