//! A small command line over the same todo file the app uses, so scripts and
//! coding agents can read and write the list.
//!
//! Arguments are parsed by hand rather than with `clap`, to keep the
//! dependency list short.

use crate::model::{Store, Todo};
use crate::store;
use serde::Serialize;
use std::process::ExitCode;

pub const USAGE: &str = "\
flodo — a floating to-do list

USAGE
    flodo                            Launch the app
    flodo list [options]             Print to-dos
    flodo add <text...> [options]    Add a to-do, print its id
    flodo done <id>...               Mark to-dos complete
    flodo undone <id>...             Mark to-dos not complete
    flodo rm <id>...                 Delete to-dos

LIST OPTIONS
    --json                           Machine-readable JSON array
    --all                            Include completed to-dos
    --count                          Print only how many matched

ADD OPTIONS
    --body <text>                    Attach a markdown body
    --json                           Print the created to-do as JSON

GLOBAL
    -h, --help                       Show this help
    -V, --version                    Show the version

NOTES
    Titles and bodies are markdown. Ids come from `flodo list --json`.
    Set FLODO_STATE_DIR to use a different data directory.
";

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// No arguments: run the GUI.
    Gui,
    Help,
    Version,
    List {
        json: bool,
        all: bool,
        count: bool,
    },
    Add {
        title: String,
        body: Option<String>,
        json: bool,
    },
    SetDone {
        ids: Vec<u64>,
        done: bool,
    },
    Remove {
        ids: Vec<u64>,
    },
}

fn parse_ids(rest: &[String]) -> Result<Vec<u64>, String> {
    if rest.is_empty() {
        return Err("expected at least one id (get them from `flodo list --json`)".into());
    }
    rest.iter()
        .map(|a| {
            a.parse::<u64>()
                .map_err(|_| format!("{a:?} is not a valid id"))
        })
        .collect()
}

pub fn parse(args: &[String]) -> Result<Command, String> {
    let Some(first) = args.first() else {
        return Ok(Command::Gui);
    };

    match first.as_str() {
        "-h" | "--help" | "help" => return Ok(Command::Help),
        "-V" | "--version" | "version" => return Ok(Command::Version),
        _ => {}
    }

    let rest = &args[1..];
    match first.as_str() {
        "list" | "ls" => {
            let (mut json, mut all, mut count) = (false, false, false);
            for a in rest {
                match a.as_str() {
                    "--json" => json = true,
                    "--all" => all = true,
                    "--count" => count = true,
                    other => return Err(format!("unknown option for `list`: {other:?}")),
                }
            }
            Ok(Command::List { json, all, count })
        }

        "add" | "new" => {
            let mut words: Vec<String> = Vec::new();
            let mut body = None;
            let mut json = false;
            let mut it = rest.iter();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--body" => body = Some(it.next().ok_or("`--body` needs a value")?.clone()),
                    "--json" => json = true,
                    other if other.starts_with("--") => {
                        return Err(format!("unknown option for `add`: {other:?}"))
                    }
                    other => words.push(other.to_string()),
                }
            }
            let title = words.join(" ").trim().to_string();
            if title.is_empty() {
                return Err("`add` needs some text".into());
            }
            Ok(Command::Add { title, body, json })
        }

        "done" | "complete" => Ok(Command::SetDone {
            ids: parse_ids(rest)?,
            done: true,
        }),
        "undone" | "uncomplete" => Ok(Command::SetDone {
            ids: parse_ids(rest)?,
            done: false,
        }),
        "rm" | "remove" | "delete" => Ok(Command::Remove {
            ids: parse_ids(rest)?,
        }),

        other => Err(format!("unknown command {other:?}")),
    }
}

/// The CLI's output contract. Deliberately a separate type from [`Todo`] so
/// internal fields (`expanded`, forward-compat `extra`) never leak into it and
/// the shape stays stable.
#[derive(Debug, Serialize, PartialEq)]
pub struct Record {
    pub id: u64,
    pub title: String,
    pub body: String,
    pub done: bool,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

impl From<&Todo> for Record {
    fn from(t: &Todo) -> Self {
        Self {
            id: t.id,
            title: t.title.clone(),
            body: t.body.clone(),
            done: t.done,
            created_at: t.created_at,
            completed_at: t.completed_at,
        }
    }
}

pub fn records(store: &Store, all: bool) -> Vec<Record> {
    store
        .todos
        .iter()
        .filter(|t| all || !t.done)
        .map(Record::from)
        .collect()
}

/// Human-readable listing. Checkbox markers keep it unambiguous and make the
/// output valid markdown.
pub fn render_text(records: &[Record]) -> String {
    if records.is_empty() {
        return "No to-dos.\n".to_string();
    }
    let mut out = String::new();
    for r in records {
        let mark = if r.done { "x" } else { " " };
        out.push_str(&format!("- [{}] {}  ({})\n", mark, r.title, r.id));
        for line in r.body.lines().filter(|l| !l.trim().is_empty()) {
            out.push_str(&format!("      {line}\n"));
        }
    }
    out
}

pub fn run(cmd: Command) -> ExitCode {
    match run_inner(cmd) {
        Ok(out) => {
            print!("{out}");
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("flodo: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn run_inner(cmd: Command) -> Result<String, String> {
    match cmd {
        Command::Gui => Ok(String::new()),
        Command::Help => Ok(USAGE.to_string()),
        Command::Version => Ok(format!("flodo {}\n", env!("CARGO_PKG_VERSION"))),

        Command::List { json, all, count } => {
            let store = store::load_todos_strict()?;
            let recs = records(&store, all);
            if count {
                return Ok(format!("{}\n", recs.len()));
            }
            if json {
                let s = serde_json::to_string_pretty(&recs).map_err(|e| e.to_string())?;
                Ok(format!("{s}\n"))
            } else {
                Ok(render_text(&recs))
            }
        }

        Command::Add { title, body, json } => {
            let mut store = store::load_todos_strict()?;
            let id = store.add(title);
            if let (Some(b), Some(t)) = (body, store.get_mut(id)) {
                t.body = b;
            }
            store::save_todos_strict(&store)?;
            let rec = Record::from(store.get(id).expect("just added"));
            if json {
                let s = serde_json::to_string_pretty(&rec).map_err(|e| e.to_string())?;
                Ok(format!("{s}\n"))
            } else {
                Ok(format!("{id}\n"))
            }
        }

        Command::SetDone { ids, done } => {
            let mut store = store::load_todos_strict()?;
            // Check every id up front so a typo in the third one doesn't leave
            // the first two already applied.
            missing(&store, &ids)?;
            for id in ids {
                if let Some(t) = store.get_mut(id) {
                    if t.done != done {
                        store.toggle(id);
                    }
                }
            }
            store::save_todos_strict(&store)?;
            Ok(String::new())
        }

        Command::Remove { ids } => {
            let mut store = store::load_todos_strict()?;
            missing(&store, &ids)?;
            for id in ids {
                store.remove(id);
            }
            store::save_todos_strict(&store)?;
            Ok(String::new())
        }
    }
}

fn missing(store: &Store, ids: &[u64]) -> Result<(), String> {
    let unknown: Vec<String> = ids
        .iter()
        .filter(|id| store.get(**id).is_none())
        .map(|id| id.to_string())
        .collect();
    if unknown.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "no to-do with id {} — nothing was changed",
            unknown.join(", ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn no_arguments_launches_the_app() {
        assert_eq!(parse(&[]).unwrap(), Command::Gui);
    }

    #[test]
    fn help_and_version_are_recognised() {
        for a in ["-h", "--help", "help"] {
            assert_eq!(parse(&args(a)).unwrap(), Command::Help, "{a}");
        }
        for a in ["-V", "--version", "version"] {
            assert_eq!(parse(&args(a)).unwrap(), Command::Version, "{a}");
        }
    }

    #[test]
    fn list_accepts_its_flags_in_any_order() {
        assert_eq!(
            parse(&args("list")).unwrap(),
            Command::List {
                json: false,
                all: false,
                count: false
            }
        );
        assert_eq!(
            parse(&args("list --all --json")).unwrap(),
            Command::List {
                json: true,
                all: true,
                count: false
            }
        );
        assert_eq!(parse(&args("ls --count")).unwrap().clone(), {
            Command::List {
                json: false,
                all: false,
                count: true,
            }
        });
    }

    #[test]
    fn add_joins_bare_words_into_one_title() {
        assert_eq!(
            parse(&args("add buy oat milk")).unwrap(),
            Command::Add {
                title: "buy oat milk".into(),
                body: None,
                json: false
            }
        );
    }

    #[test]
    fn add_takes_a_body_and_keeps_markdown_intact() {
        let a = vec![
            "add".to_string(),
            "Fix `login_test`".to_string(),
            "--body".to_string(),
            "Races on the **cookie**.".to_string(),
        ];
        assert_eq!(
            parse(&a).unwrap(),
            Command::Add {
                title: "Fix `login_test`".into(),
                body: Some("Races on the **cookie**.".into()),
                json: false
            }
        );
    }

    #[test]
    fn add_without_text_is_an_error() {
        assert!(parse(&args("add")).is_err());
        assert!(parse(&args("add    ")).is_err());
    }

    #[test]
    fn a_body_flag_with_no_value_is_an_error() {
        assert!(parse(&args("add thing --body")).is_err());
    }

    #[test]
    fn done_undone_and_rm_parse_id_lists() {
        assert_eq!(
            parse(&args("done 1 2 3")).unwrap(),
            Command::SetDone {
                ids: vec![1, 2, 3],
                done: true
            }
        );
        assert_eq!(
            parse(&args("undone 7")).unwrap(),
            Command::SetDone {
                ids: vec![7],
                done: false
            }
        );
        assert_eq!(
            parse(&args("rm 9")).unwrap(),
            Command::Remove { ids: vec![9] }
        );
    }

    #[test]
    fn bad_ids_and_unknown_commands_are_rejected() {
        assert!(parse(&args("done")).is_err());
        assert!(parse(&args("done abc")).is_err());
        assert!(parse(&args("rm -1")).is_err());
        assert!(parse(&args("frobnicate")).is_err());
        assert!(parse(&args("list --nope")).is_err());
    }

    fn sample() -> Store {
        let mut s = Store::default();
        s.add("second");
        let done = s.add("first");
        s.toggle(done);
        s
    }

    #[test]
    fn records_hide_completed_unless_asked() {
        let s = sample();
        assert_eq!(records(&s, false).len(), 1);
        assert_eq!(records(&s, true).len(), 2);
        assert_eq!(records(&s, false)[0].title, "second");
    }

    #[test]
    fn json_output_has_a_stable_shape() {
        let s = sample();
        let json = serde_json::to_value(records(&s, true)).unwrap();
        let first = &json[0];
        for key in ["id", "title", "body", "done", "created_at", "completed_at"] {
            assert!(first.get(key).is_some(), "missing {key} in {first}");
        }
        // Internal-only fields must never leak into the CLI contract.
        for key in ["expanded", "extra", "version"] {
            assert!(first.get(key).is_none(), "{key} leaked into CLI output");
        }
    }

    #[test]
    fn text_output_is_valid_markdown_checkboxes() {
        let s = sample();
        let out = render_text(&records(&s, true));
        assert!(out.contains("- [x] first"), "{out}");
        assert!(out.contains("- [ ] second"), "{out}");
    }

    #[test]
    fn text_output_indents_body_lines_under_their_todo() {
        let mut s = Store::default();
        let id = s.add("with body");
        s.get_mut(id).unwrap().body = "line one\n\nline two".into();
        let out = render_text(&records(&s, false));
        assert!(out.contains("      line one"), "{out}");
        assert!(out.contains("      line two"), "{out}");
    }

    #[test]
    fn an_empty_list_says_so_rather_than_printing_nothing() {
        assert_eq!(render_text(&[]), "No to-dos.\n");
    }

    #[test]
    fn missing_ids_are_reported_before_anything_is_changed() {
        let s = sample();
        let real = s.todos[0].id;
        assert!(missing(&s, &[real]).is_ok());
        let err = missing(&s, &[real, 424242]).unwrap_err();
        assert!(err.contains("424242"), "{err}");
        assert!(err.contains("nothing was changed"), "{err}");
    }
}
