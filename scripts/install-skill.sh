#!/usr/bin/env sh
# Install the Flodo skill so Claude knows how to use the `flodo` CLI.
#
#   ./scripts/install-skill.sh              install globally (~/.claude/skills)
#   ./scripts/install-skill.sh --project    install into ./.claude/skills
#   ./scripts/install-skill.sh --link       symlink instead of copy
#   ./scripts/install-skill.sh --uninstall  remove it again
#
# Installing is entirely optional — Flodo works fine without it.

set -eu

src=$(CDPATH= cd -- "$(dirname -- "$0")/../skills/flodo" && pwd)
dest_root="${HOME}/.claude/skills"
mode=copy
action=install

for arg in "$@"; do
    case "$arg" in
        --project) dest_root="$(pwd)/.claude/skills" ;;
        --link) mode=link ;;
        --uninstall) action=uninstall ;;
        -h | --help)
            sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "install-skill: unknown option $arg" >&2
            exit 1
            ;;
    esac
done

dest="${dest_root}/flodo"

if [ "$action" = uninstall ]; then
    if [ -e "$dest" ] || [ -L "$dest" ]; then
        rm -rf "$dest"
        echo "Removed $dest"
    else
        echo "Nothing installed at $dest"
    fi
    exit 0
fi

if [ ! -f "${src}/SKILL.md" ]; then
    echo "install-skill: can't find ${src}/SKILL.md" >&2
    exit 1
fi

mkdir -p "$dest_root"
rm -rf "$dest"

if [ "$mode" = link ]; then
    ln -s "$src" "$dest"
    echo "Linked $dest -> $src"
else
    cp -R "$src" "$dest"
    echo "Installed $dest"
fi

if ! command -v flodo >/dev/null 2>&1; then
    echo
    echo "Note: \`flodo\` is not on your PATH yet, so the skill won't be able to"
    echo "run it. Either install a release build, or:"
    echo
    echo "    cargo install --path ."
fi
