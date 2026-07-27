//! The entire data model. Pure logic, no IO — everything here is unit-tested.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::time::{SystemTime, UNIX_EPOCH};

pub const STORE_VERSION: u32 = 1;

fn default_version() -> u32 {
    STORE_VERSION
}

/// Seconds since the Unix epoch. Avoids a `chrono` dependency.
pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Todo {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub completed_at: Option<i64>,
    #[serde(default)]
    pub expanded: bool,
    /// Catches keys written by a newer version so we round-trip instead of
    /// destroying them.
    #[serde(flatten, default, skip_serializing_if = "Map::is_empty")]
    pub extra: Map<String, Value>,
}

impl Todo {
    pub fn new(id: u64, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            body: String::new(),
            done: false,
            created_at: now_secs(),
            completed_at: None,
            expanded: false,
            extra: Map::new(),
        }
    }

    pub fn has_body(&self) -> bool {
        !self.body.trim().is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Store {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub todos: Vec<Todo>,
    #[serde(flatten, default, skip_serializing_if = "Map::is_empty")]
    pub extra: Map<String, Value>,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            version: STORE_VERSION,
            todos: Vec::new(),
            extra: Map::new(),
        }
    }
}

impl Store {
    /// Monotonic and unique: timestamp-derived so ids stay meaningful across
    /// sessions, but always at least `max_existing + 1` so a clock that jumps
    /// backwards can never produce a collision.
    fn next_id(&self) -> u64 {
        let from_clock = now_millis() << 12;
        let from_data = self.todos.iter().map(|t| t.id).max().map_or(0, |m| m + 1);
        from_clock.max(from_data)
    }

    /// Appends to the bottom, matching where the composer sits.
    pub fn add(&mut self, title: impl Into<String>) -> u64 {
        let id = self.next_id();
        self.todos.push(Todo::new(id, title));
        id
    }

    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.todos.iter().position(|t| t.id == id)
    }

    pub fn get(&self, id: u64) -> Option<&Todo> {
        self.todos.iter().find(|t| t.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Todo> {
        self.todos.iter_mut().find(|t| t.id == id)
    }

    pub fn toggle(&mut self, id: u64) {
        if let Some(t) = self.get_mut(id) {
            t.done = !t.done;
            t.completed_at = if t.done { Some(now_secs()) } else { None };
        }
    }

    /// Returns the removed todo and where it was, so undo can put it back.
    pub fn remove(&mut self, id: u64) -> Option<(usize, Todo)> {
        let ix = self.index_of(id)?;
        Some((ix, self.todos.remove(ix)))
    }

    pub fn restore(&mut self, index: usize, todo: Todo) {
        let ix = index.min(self.todos.len());
        self.todos.insert(ix, todo);
    }

    /// Moves a todo to an absolute position. Out-of-range targets clamp rather
    /// than panic, so callers can be sloppy.
    pub fn move_to(&mut self, id: u64, to: usize) {
        let Some(from) = self.index_of(id) else {
            return;
        };
        let to = to.min(self.todos.len().saturating_sub(1));
        if from == to {
            return;
        }
        let t = self.todos.remove(from);
        self.todos.insert(to, t);
    }

    pub fn move_up(&mut self, id: u64) {
        if let Some(ix) = self.index_of(id) {
            if ix > 0 {
                self.todos.swap(ix, ix - 1);
            }
        }
    }

    pub fn move_down(&mut self, id: u64) {
        if let Some(ix) = self.index_of(id) {
            if ix + 1 < self.todos.len() {
                self.todos.swap(ix, ix + 1);
            }
        }
    }

    pub fn any_completed(&self) -> bool {
        self.todos.iter().any(|t| t.done)
    }

    /// Ids in display order, honouring the hide-completed toggle.
    pub fn visible_ids(&self, hide_completed: bool) -> Vec<u64> {
        self.todos
            .iter()
            .filter(|t| !(hide_completed && t.done))
            .map(|t| t.id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_with(n: usize) -> Store {
        let mut s = Store::default();
        for i in 0..n {
            s.add(format!("task {i}"));
        }
        s
    }

    #[test]
    fn add_appends_and_ids_are_unique_and_increasing() {
        let s = store_with(50);
        assert_eq!(s.todos.len(), 50);
        assert_eq!(s.todos[0].title, "task 0");
        assert_eq!(s.todos[49].title, "task 49");
        for w in s.todos.windows(2) {
            assert!(w[1].id > w[0].id, "ids must strictly increase");
        }
    }

    #[test]
    fn ids_stay_unique_after_reload_even_if_clock_went_backwards() {
        // Simulates a file written far in the future, then reopened.
        let mut s = Store::default();
        s.todos.push(Todo::new(u64::MAX / 2, "from the future"));
        let id = s.add("now");
        assert!(id > u64::MAX / 2);
    }

    #[test]
    fn toggle_sets_and_clears_completed_at() {
        let mut s = store_with(1);
        let id = s.todos[0].id;
        s.toggle(id);
        assert!(s.get(id).unwrap().done);
        assert!(s.get(id).unwrap().completed_at.is_some());
        s.toggle(id);
        assert!(!s.get(id).unwrap().done);
        assert!(s.get(id).unwrap().completed_at.is_none());
    }

    #[test]
    fn remove_then_restore_puts_it_back_where_it_was() {
        let mut s = store_with(5);
        let id = s.todos[2].id;
        let (ix, todo) = s.remove(id).unwrap();
        assert_eq!(ix, 2);
        assert_eq!(s.todos.len(), 4);
        s.restore(ix, todo);
        assert_eq!(s.todos.len(), 5);
        assert_eq!(s.todos[2].id, id);
    }

    #[test]
    fn restore_past_the_end_clamps_instead_of_panicking() {
        let mut s = store_with(2);
        s.restore(99, Todo::new(1, "x"));
        assert_eq!(s.todos.len(), 3);
        assert_eq!(s.todos[2].title, "x");
    }

    #[test]
    fn move_up_and_down_are_no_ops_at_the_boundaries() {
        let mut s = store_with(3);
        let (first, last) = (s.todos[0].id, s.todos[2].id);
        s.move_up(first);
        assert_eq!(s.todos[0].id, first);
        s.move_down(last);
        assert_eq!(s.todos[2].id, last);
    }

    #[test]
    fn move_down_then_up_returns_to_start() {
        let mut s = store_with(3);
        let id = s.todos[0].id;
        s.move_down(id);
        assert_eq!(s.index_of(id), Some(1));
        s.move_up(id);
        assert_eq!(s.index_of(id), Some(0));
    }

    #[test]
    fn move_to_clamps_and_reorders() {
        let mut s = store_with(4);
        let id = s.todos[0].id;
        s.move_to(id, 99);
        assert_eq!(s.index_of(id), Some(3));
        s.move_to(id, 0);
        assert_eq!(s.index_of(id), Some(0));
    }

    #[test]
    fn mutations_on_a_missing_id_do_nothing() {
        let mut s = store_with(2);
        let before = s.todos.clone();
        s.toggle(999);
        s.move_up(999);
        s.move_down(999);
        s.move_to(999, 0);
        assert!(s.remove(999).is_none());
        assert_eq!(s.todos, before);
    }

    #[test]
    fn visible_ids_respects_hide_completed() {
        let mut s = store_with(3);
        let mid = s.todos[1].id;
        s.toggle(mid);
        assert_eq!(s.visible_ids(false).len(), 3);
        let shown = s.visible_ids(true);
        assert_eq!(shown.len(), 2);
        assert!(!shown.contains(&mid));
        assert!(s.any_completed());
    }

    #[test]
    fn unknown_fields_survive_a_round_trip() {
        let json = r#"{
            "version": 99,
            "todos": [{
                "id": 7, "title": "hi", "someFutureField": {"a": 1}
            }],
            "topLevelFuture": [1, 2, 3]
        }"#;
        let s: Store = serde_json::from_str(json).unwrap();
        assert_eq!(s.version, 99);
        assert_eq!(s.todos[0].title, "hi");

        let out = serde_json::to_string(&s).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["topLevelFuture"], serde_json::json!([1, 2, 3]));
        assert_eq!(v["todos"][0]["someFutureField"]["a"], 1);
    }

    #[test]
    fn missing_optional_fields_use_defaults() {
        let s: Store = serde_json::from_str(r#"{"todos":[{"id":1,"title":"a"}]}"#).unwrap();
        assert_eq!(s.version, STORE_VERSION);
        let t = &s.todos[0];
        assert!(!t.done && !t.expanded && t.body.is_empty() && t.completed_at.is_none());
    }

    #[test]
    fn has_body_ignores_whitespace_only_bodies() {
        let mut t = Todo::new(1, "x");
        assert!(!t.has_body());
        t.body = "   \n\t ".into();
        assert!(!t.has_body());
        t.body = "note".into();
        assert!(t.has_body());
    }
}
