//! Random nesting, completing (repeats too), moving, deleting, undo and sync on two stores: views
//! never fail and never show a task twice (an app list keyed by task id would crash on it).

use not_core::date::Date;
use not_core::{INBOX, Store, TODAY, UPCOMING};
mod common;
use common::Log;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn pick(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

#[test]
fn nested_edits_never_break_views() {
    for seed in 1..60u64 {
        let mut r = Rng(seed * 7919 + 1);
        let a = Store::open_in_memory().unwrap();
        let b = Store::open_in_memory().unwrap();
        let mut log = Log::default();
        let today = Date::parse("2026-11-08").unwrap();
        let views = [INBOX, TODAY, UPCOMING, "#home"];
        for step in 0..80 {
            let s = if r.pick(2) == 0 { &a } else { &b };
            let ids: Vec<String> = s.all_tasks().unwrap().into_iter().map(|t| t.id).collect();
            let any = |r: &mut Rng| {
                if ids.is_empty() {
                    None
                } else {
                    Some(ids[r.pick(ids.len())].clone())
                }
            };
            let op = r.pick(14);
            let res: Result<(), String> = (|| {
                match op {
                    0 => {
                        s.quick_add(
                            INBOX,
                            [
                                "t every day",
                                "x #home",
                                "y today",
                                "z tomorrow every 2 days",
                                "w",
                            ][r.pick(5)],
                            today,
                            &[],
                        )
                        .map_err(|e| e.to_string())?;
                    }
                    1 => {
                        if let Some(p) = any(&mut r) {
                            s.add_task(&p, ["sub", "sub every week"][r.pick(2)], today)
                                .map_err(|e| e.to_string())?;
                        }
                    }
                    2 | 3 => {
                        if let Some(t) = any(&mut r) {
                            s.set_completed(&t, r.pick(3) != 0, today)
                                .map_err(|e| e.to_string())?;
                        }
                    }
                    4 => {
                        if let Some(t) = any(&mut r) {
                            let p = any(&mut r);
                            let _ = s.nest_in_view(
                                views[r.pick(4)],
                                &t,
                                p.as_deref(),
                                any(&mut r).as_deref(),
                                today,
                            );
                        }
                    }
                    5 => {
                        if let Some(t) = any(&mut r) {
                            let _ = s.indent_in_view(views[r.pick(4)], &t, today);
                        }
                    }
                    6 => {
                        if let Some(t) = any(&mut r) {
                            let _ = s.outdent_in_view(views[r.pick(4)], &t);
                        }
                    }
                    7 => {
                        if let Some(t) = any(&mut r) {
                            let _ = s.delete(&t);
                        }
                    }
                    8 => {
                        let _ = s.undo();
                    }
                    9 => {
                        let _ = s.redo();
                    }
                    10 => {
                        if let Some(t) = any(&mut r) {
                            let _ = s.move_in_view(views[r.pick(4)], &t, any(&mut r).as_deref(), today);
                        }
                    }
                    11 => {
                        log.sync(&a);
                        log.sync(&b);
                    }
                    12 => {
                        if let Some(t) = any(&mut r) {
                            let _ = s.move_task_before(&t, any(&mut r).as_deref(), any(&mut r).as_deref());
                        }
                    }
                    _ => {
                        if let Some(t) = any(&mut r) {
                            let _ = s.subtasks(&t);
                            let _ = s.task_by_id(&t);
                        }
                    }
                }
                Ok(())
            })();
            res.unwrap_or_else(|e| assert!(!e.is_empty()));
            for v in views {
                let o = s
                    .view_outline(v, today)
                    .unwrap_or_else(|e| panic!("seed {seed} step {step} op {op}: {e}"));
                let mut seen = std::collections::HashSet::new();
                for row in o.open.iter().chain(o.completed.iter()) {
                    assert!(
                        seen.insert(row.task.id.clone()),
                        "seed {seed} step {step} op {op}: dup {} in {v}",
                        row.task.title
                    );
                }
                let _ = s.view_later(v, today).unwrap();
            }
            s.views(today).unwrap();
        }
    }
}
