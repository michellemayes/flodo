<div align="center">

# Flodo

A small floating to-do list for macOS, Linux, and Windows.

[![CI](https://github.com/michellemayes/flodo/actions/workflows/ci.yml/badge.svg)](https://github.com/michellemayes/flodo/actions/workflows/ci.yml)
[![Release](https://github.com/michellemayes/flodo/actions/workflows/release.yml/badge.svg)](https://github.com/michellemayes/flodo/actions/workflows/release.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Rust](https://img.shields.io/badge/rust-stable-orange.svg)
![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey.svg)

<img src="docs/images/hero.png" alt="Flodo showing a short to-do list with one item checked off" width="380">

</div>

---

Flodo is a frameless panel that floats above your other windows. It holds one
list: add a to-do, check it off, and hide the completed ones when you want to.

It has no tags, priorities, due dates, projects, or sub-tasks, and is not
intended to grow them.

## What it does

| Area | Detail |
|---|---|
| Window | Frameless and always-on-top. Drag it by any empty space; unpin it when it's in the way. |
| Bodies | A to-do is one line, but can carry a collapsible markdown description underneath, including fenced code snippets. |
| Appearance | Eight accent colours, light and dark, plus font, code font, text size, row spacing, and opacity. |
| Keyboard | The composer keeps focus after <kbd>Enter</kbd>, so several to-dos can be added without using the mouse. |
| Size | A single binary, around 8 MB. No webview, no background service, no account. |
| Storage | Two JSON files you can read, edit, and sync. |
| Scripting | A CLI over the same list, and an optional Claude skill for agents. |

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

Type in the box at the top and press <kbd>Enter</kbd>. The new to-do appears on
the line directly below, and the field keeps focus, so adding several in a row
is uninterrupted typing.

Click the circle to check something off. Completed to-dos stay in place, dimmed
and struck through, so the list doesn't reorder under the cursor. The eye in the
title bar hides them.

<div align="center">
<img src="docs/images/light.png" alt="Flodo in light mode with a blue accent" width="340">
</div>

### Markdown

Titles and bodies are markdown. While you're typing you see the raw source;
click away and it renders. There is no formatting toolbar.

| While you're typing | After you click away |
|---|---|
| <img src="docs/images/editing.png" alt="A to-do being edited, showing raw markdown asterisks" width="330"> | <img src="docs/images/rendered.png" alt="The same to-do rendered, with bold and inline code" width="330"> |

### Bodies

Hover a row and click the chevron to add a body. It holds the detail: a note, a
link, a stack trace, a command.

<div align="center">
<img src="docs/images/markdown.png" alt="A to-do expanded to show a markdown body with a heading, a link, a Rust code block, nested lists and a blockquote" width="380">
</div>

Bodies support headings, **bold**, *italic*, `inline code`, fenced code blocks
with a language label and a copy button, nested lists, links, blockquotes,
horizontal rules, and ~~strikethrough~~.

Code blocks scroll sideways rather than wrapping:

````markdown
Races on the session cookie. Reproduce with:

```sh
cargo test --test login -- --test-threads=1
```

- [the flaky run](https://ci.example.com/12345)
- probably the `SameSite` change
````

### Appearance

<div align="center">
<img src="docs/images/settings.png" alt="The settings sheet showing accent swatches, appearance, font pickers and sliders" width="340">
</div>

Seven settings on one screen, opened with <kbd>⌘</kbd><kbd>,</kbd>.

The accent colour tints the whole panel, not just the checkbox: background,
surfaces, and borders all shift toward its hue at low saturation.

| Pink · dark | Green · light | Amber · dark | Purple · light |
|---|---|---|---|
| <img src="docs/images/accent-pink.png" alt="Flodo with a pink accent in dark mode" width="220"> | <img src="docs/images/accent-green.png" alt="Flodo with a green accent in light mode" width="220"> | <img src="docs/images/accent-amber.png" alt="Flodo with an amber accent in dark mode" width="220"> | <img src="docs/images/accent-purple.png" alt="Flodo with a purple accent in light mode" width="220"> |

All eight accents are contrast-tested in both light and dark. A unit test
asserts WCAG AA for body text against the background.

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

## Command line

The same binary is also a CLI over the same list, for scripts and coding
agents.

```console
$ flodo add Buy oat milk
7312124937646080

$ flodo add "Fix the flaky login_test" --body "Races on the session cookie."
7312124937695232

$ flodo list
- [ ] Fix the flaky login_test  (7312124937695232)
      Races on the session cookie.
- [ ] Buy oat milk  (7312124937646080)

$ flodo done 7312124937646080

$ flodo list --count
1
```

| Command | What it does |
|---|---|
| `flodo list` | Open to-dos as markdown checkboxes |
| `flodo list --json` | Machine-readable array |
| `flodo list --all` | Include completed |
| `flodo list --count` | Just the number |
| `flodo add <text> [--body <text>]` | Add one, print its id |
| `flodo done <id>...` | Mark complete |
| `flodo undone <id>...` | Mark not complete |
| `flodo rm <id>...` | Delete |

`--json` gives a stable record shape — internal fields never leak into it:

```json
[
  {
    "id": 7312124937695232,
    "title": "Fix the flaky `login_test`",
    "body": "Races on the session cookie.",
    "done": false,
    "created_at": 1785186752,
    "completed_at": null
  }
]
```

Writes are safe while the app is open: it polls the file and picks up outside
edits within a second, so it won't overwrite what the CLI wrote. Two further
guarantees:

- **Unknown ids change nothing.** `flodo done 1 2 999` with one bad id exits
  non-zero having applied none of them, rather than leaving the first two
  half-done.
- **A file that fails to parse is never written over.** The CLI exits non-zero
  instead of starting from an empty list and overwriting the real one.

## Claude skill

Optional, installed with one command. It documents the CLI for Claude:
fetching ids from `--json` before changing anything, and asking rather than
guessing when a title is ambiguous.

```sh
./scripts/install-skill.sh            # global: ~/.claude/skills/flodo
./scripts/install-skill.sh --project  # this repo only: ./.claude/skills/flodo
./scripts/install-skill.sh --link     # symlink, so it tracks the repo
./scripts/install-skill.sh --uninstall
```

Then just ask, in Claude Code or the Claude app:

> what's on my to-do list?
>
> add "renew the domain" to my list
>
> mark the dentist one done

The skill is a single file, [`skills/flodo/SKILL.md`](skills/flodo/SKILL.md),
so you can read what Claude is being told before installing it. Flodo does not
require it.

> [!NOTE]
> The skill runs `flodo`, so the binary needs to be on your `PATH`
> (`cargo install --path .` does that). The installer warns you if it isn't.

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

It is an ordinary file: edit it by hand, keep it in a git repo, or sync it.
`FLODO_STATE_DIR` points Flodo somewhere else.

Three properties protect it:

- Saves are **atomic**: written to a temp file and renamed into place, so a
  crash mid-write cannot leave a partial list.
- A file that fails to parse is **quarantined**, never overwritten. The
  original bytes are kept as `todos.json.corrupt-<timestamp>`, and the app
  shows a notice.
- Unknown fields **round-trip**, so an older build will not strip fields a
  newer one wrote.

## Known limitations

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
- **macOS builds are not notarized**, which is why the first launch needs
  right-click → Open.

## Development

```sh
cargo test                                                  # 82 tests
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Tests cover what can be checked without a screen: the model, atomic writes and
corruption handling, settings clamping, the markdown parser (including a
no-panic sweep over pathological input), CLI argument parsing and output shape,
hotkey parsing, font validation, and palette contrast.

The GUI is checked by screenshot. `eframe` has a built-in hook that renders a
couple of frames, writes a PNG, and exits, so nothing beyond Xvfb is needed:

```sh
xvfb-run -a -s "-screen 0 700x900x24" \
  env LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
      FLODO_STATE_DIR=/tmp/flodo-shots \
      FLODO_DEMO=showcase \
      EFRAME_SCREENSHOT_TO=/tmp/flodo.png \
  cargo run --features screenshot
```

`FLODO_DEMO` seeds a scenario in memory without touching the real list:
`hero`, `showcase`, `editing`, `rendered`, `settings`, `empty`, `long`, `body`.
Every image in this README was produced this way.

> [!NOTE]
> The build uses the glow backend rather than wgpu. eframe's screenshot hook is
> glow-only, and wgpu needs a Vulkan or GLES adapter that headless CI often
> lacks, so switching backends would cost this screenshot workflow.

### Releasing

Push a semver tag:

```sh
git tag -a v0.1.0 -m "Flodo v0.1.0"
git push origin v0.1.0
```

This builds a universal macOS `.app` (arm64 + x86_64, ad-hoc signed) plus Linux
and Windows archives, and publishes them to a GitHub Release with
`SHA256SUMS.txt`. Tags containing a hyphen, such as `v0.1.0-rc.1`, publish as
pre-releases.

## Built with

[egui](https://github.com/emilk/egui) ·
[eframe](https://github.com/emilk/egui/tree/master/crates/eframe) ·
[pulldown-cmark](https://github.com/raphlinus/pulldown-cmark) ·
[fontdb](https://github.com/RazrFalcon/fontdb) ·
[skrifa](https://github.com/googlefonts/fontations) ·
[global-hotkey](https://github.com/tauri-apps/global-hotkey) ·
[serde](https://serde.rs)

## License

[MIT](LICENSE)
