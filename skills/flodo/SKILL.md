---
name: flodo
description: Read and write the user's Flodo to-do list through the `flodo` CLI. Use whenever the user asks what's on their to-do list, what they need to do, what's outstanding or still open, or asks to add / capture / note down a task, tick something off, mark it done, or delete it. Also use when the user asks you to record follow-up work you just identified. Triggers on "my to-dos", "my list", "add to my list", "what do I still need to do", "mark that done", "flodo".
---

# Flodo

Flodo is a small floating to-do app. Its list lives in a plain JSON file, and
the `flodo` binary is both the app and a CLI over that same file.

## Check it is available

```sh
flodo --version
```

If that fails, Flodo isn't installed or isn't on `PATH`. Say so rather than
guessing at a data file — do not go hunting for `todos.json`.

## Reading

Always use `--json` when you need to act on the result; the plain text form is
for showing a human.

```sh
flodo list --json          # open to-dos only
flodo list --json --all    # include completed
flodo list --count         # just the number of open to-dos
flodo list                 # human-readable, markdown checkboxes
```

Each record has a stable shape:

```json
{
  "id": 7312124937695232,
  "title": "Fix the flaky `login_test`",
  "body": "Races on the session cookie.",
  "done": false,
  "created_at": 1785186752,
  "completed_at": null
}
```

`title` and `body` are **markdown source**. If you are showing them to the user,
render or strip the markup rather than reading `**` aloud.

## Writing

```sh
flodo add Buy oat milk                          # prints the new id
flodo add "Fix the flaky login_test" \
  --body "Races on the session cookie."         # optional markdown body
flodo add "Ship it" --json                      # print the created record

flodo done   <id>...                            # mark complete
flodo undone <id>...                            # mark not complete
flodo rm     <id>...                            # delete
```

Ids come from `flodo list --json`. They are large integers — never invent one,
and never guess at an id from a title.

## Rules

1. **Get the id first.** To change or remove something, run `flodo list --json`,
   match on `title`, then use that record's `id`.
2. **Ambiguous match, ask.** If more than one to-do plausibly matches what the
   user described, list the candidates and ask which they meant. Do not guess —
   `rm` has no confirmation prompt.
3. **Never edit `todos.json` directly.** The CLI writes atomically and refuses
   to run against a file it cannot parse. Hand-editing while the app is open
   risks losing the edit.
4. **Deleting is not completing.** "I finished X" means `flodo done`. Only use
   `rm` when the user actually wants the item gone.
5. **One to-do per task.** Flodo has no tags, priorities, due dates, or
   sub-tasks by design. Don't try to encode them in the title — put detail in
   the `--body` instead.
6. **Check the exit code.** Non-zero means nothing was changed; the reason is on
   stderr. Report it rather than retrying blindly.

## Examples

**"What's on my list?"**

```sh
flodo list --json
```
Then summarise the open items in your own words.

**"Add a reminder to renew the domain"**

```sh
flodo add "Renew the domain"
```

**"I finished the dentist thing"**

```sh
flodo list --json          # find the record whose title mentions the dentist
flodo done 7312124937674752
```

**Capturing follow-up work you found while coding**

```sh
flodo add "Fix the N+1 query in the orders report" \
  --body "Spotted in \`OrdersController#index\`. Each row re-queries the customer."
```

## Notes

- Set `FLODO_STATE_DIR` to point at a different data directory; the CLI and the
  app both honour it.
- Writes show up in the running app within about a second — no restart needed.
- On Windows, release builds are GUI-subsystem and print nothing to a console.
  The CLI is intended for macOS and Linux.
