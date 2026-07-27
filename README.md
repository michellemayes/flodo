# Flodo

A floating to-do list. One infinite list, a checkbox, and a show/hide toggle for
what you've finished. That's the whole app.

It is deliberately **not** a project management tool. No tags, no priorities, no
due dates, no projects, no sub-tasks.

Two things make it more than a checkbox list:

- **A to-do can carry a body.** One line by default, but any to-do can hold a
  markdown description underneath it — including fenced code snippets —
  collapsed until you want it.
- **It's yours to style.** Accent colour, font, code font, text size, row
  spacing, and opacity are all yours to pick.

Built in Rust with [egui](https://github.com/emilk/egui). Frameless,
always-on-top, ~7 MB.

## Install

Grab the latest [release](../../releases). On macOS, unzip and drag `Flodo.app`
to Applications. Builds are ad-hoc signed but not notarized, so the first launch
needs right-click → **Open**, or:

```sh
xattr -dr com.apple.quarantine /Applications/Flodo.app
```

Or build it yourself:

```sh
cargo run --release
```

## Using it

Type in the box at the top and press Enter. The new to-do lands on the line
directly below, and the field keeps focus so you can type straight into the next
one.

Click the circle to check something off — it stays where it is, dimmed and
struck through, until you hide completed items with the eye.

Click a title to edit it. Titles and bodies are markdown: while you're typing you
see the raw source, and when you click away it renders.

| | |
|---|---|
| `Enter` | Add the to-do at the top, stay focused for the next one |
| `Cmd+N` | Jump to the composer |
| `Cmd+E` | Show/hide completed |
| `Cmd+P` | Pin / unpin from always-on-top |
| `Cmd+,` | Settings |
| `Cmd+Z` | Undo the last delete |
| `Cmd+↑` / `Cmd+↓` | Move the to-do you're editing |
| `Cmd+Backspace` | Delete |
| `Esc` | Stop editing, or close settings |
| `Alt+Space` | Summon or hide Flodo from anywhere |

Drag the window by any empty background. Drag the handle on the left of a row to
reorder it.

## Bodies

A chevron appears on a row that has a body; hover any row to add one. Bodies
support **bold**, *italic*, `inline code`, fenced code blocks with a language
tag and a copy button, lists (nested), links, blockquotes, headings, and
strikethrough. Code blocks scroll sideways rather than wrapping, because
wrapped code is unreadable.

Everything renders in your chosen font, size, and accent — that's why the
markdown renderer is hand-rolled rather than borrowed.

## Where your data lives

| | |
|---|---|
| macOS | `~/Library/Application Support/Flodo/` |
| Linux | `~/.local/share/flodo/` |
| Windows | `%APPDATA%\Flodo\` |

Two plain JSON files: `todos.json` and `settings.json`. Both are hand-editable
and sync-friendly. Writes go to a temp file and are renamed into place, so a
crash mid-write can't corrupt the list, and a file that fails to parse is moved
aside as `todos.json.corrupt-<timestamp>` rather than overwritten.

Set `FLODO_STATE_DIR` to point somewhere else.

## Known limitations

- **Emoji render in monochrome.** epaint has no COLR/sbix path, so Apple Colour
  Emoji falls back to a bundled monochrome font.
- **Bold and italic need real faces.** egui has no synthetic emphasis. Flodo
  picks up a family's bold/italic/oblique faces, and can instance a variable
  `wght` or `slnt` axis where one exists (this covers SF Pro and Inter). A family
  with no bold face at all renders `**bold**` as regular.
- **Text editing is egui's, not the system's** — no spellcheck, no emoji picker,
  no dictation, and only partial IME support.
- Flodo appears in the Dock and in Cmd-Tab. Running it as a menu-bar-only
  accessory is a plausible future change, not a current one.

## Development

```sh
cargo test                    # 61 tests, all pure logic
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

The GUI is verified by screenshot. `eframe`'s built-in hook renders a few frames,
writes a PNG, and exits — no display server tooling required beyond Xvfb:

```sh
xvfb-run -a -s "-screen 0 480x760x24" \
  env LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
      FLODO_STATE_DIR=/tmp/flodo-shots \
      FLODO_DEMO=body \
      EFRAME_SCREENSHOT_TO=/tmp/flodo.png \
  cargo run --features screenshot
```

`FLODO_DEMO` seeds a scenario without touching real data:
`basic`, `body`, `editing`, `settings`, `empty`, `long`.

Note the glow backend is not incidental — the screenshot hook is glow-only, and
wgpu needs a Vulkan or GLES adapter that headless CI often lacks.

### Releasing

Push a semver tag:

```sh
git tag -a v0.1.0 -m "Flodo v0.1.0"
git push origin v0.1.0
```

That builds a universal macOS `.app` (arm64 + x86_64), plus Linux and Windows
archives, and publishes them to a GitHub Release with `SHA256SUMS.txt`. Tags
containing a hyphen (`v0.1.0-rc.1`) publish as pre-releases.

## Licence

MIT
