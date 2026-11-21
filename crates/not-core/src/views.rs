//! Views (ADR 0014): the built-in Inbox, Today, Upcoming and Someday, saved views (smart lists the
//! user writes as [`crate::filter`] queries), their order and sort, tags, and the migration of
//! lists from earlier versions into tags and saved views.
//!
//! Anywhere a view id is taken, a query starting with `#` works too (`"#groceries"`), for browsing
//! a tag without saving a view for it.

use std::collections::{HashMap, HashSet};

use loro::{LoroMap, LoroValue, TreeParentId, ValueOrContainer};

use crate::checklist::MIGRATE_ORIGIN;
use crate::date::Date;
use crate::filter::{Candidate, Defaults, Query, rename_tag_in};
use crate::parse::normalize_tag;
use crate::projection::{self, DELETED_AT, KIND_LIST, KIND_TASK, TAGS, TITLE, meta_i64, meta_str};
use crate::store::{
    ARCHIVE_AFTER_MS, INBOX, Outline, SOMEDAY_VIEW, Store, TODAY, Task, TaskSort, UPCOMING, now_millis,
    push_subtree, tree_id, write_tags,
};
use crate::{Error, Result};

const QUERY: &str = "query";
const POSITION: &str = "position";

const BUILT_IN: [(&str, &str, &str); 4] = [
    (INBOX, "Inbox", "untagged"),
    (TODAY, "Today", "today"),
    (UPCOMING, "Upcoming", "upcoming"),
    (SOMEDAY_VIEW, "Someday", "someday"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub id: String,
    pub title: String,
    /// Empty for built-in views.
    pub query: String,
    pub built_in: bool,
    pub sort: TaskSort,
    /// Open tasks it shows at the top level.
    pub open_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagInfo {
    pub name: String,
    /// Open tasks carrying it themselves.
    pub open_count: u32,
}

/// Every task with what it inherits, for matching.
struct Context {
    tasks: Vec<Task>,
    index: HashMap<String, usize>,
    /// Own tags plus ancestors'.
    tags: Vec<Vec<String>>,
    deferred: Vec<bool>,
    /// An ancestor is completed.
    under_done: Vec<bool>,
}

impl Context {
    fn new(tasks: Vec<Task>, today: Date) -> Self {
        let index: HashMap<String, usize> =
            tasks.iter().enumerate().map(|(i, t)| (t.id.clone(), i)).collect();
        let n = tasks.len();
        let mut ctx = Self {
            tasks,
            index,
            tags: vec![Vec::new(); n],
            deferred: vec![false; n],
            under_done: vec![false; n],
        };
        let mut done = vec![false; n];
        for i in 0..n {
            ctx.resolve(i, today, &mut done);
        }
        ctx
    }

    fn resolve(&mut self, i: usize, today: Date, done: &mut [bool]) {
        if done[i] {
            return;
        }
        done[i] = true;
        let task = &self.tasks[i];
        let date = |d: &Option<String>| d.as_deref().and_then(|d| Date::parse(d).ok());
        let (start, deadline) = (date(&task.start), date(&task.deadline));
        // A deadline that has come makes it show, whatever its start date.
        let own_deferred =
            (start.is_some_and(|s| s > today) || task.someday) && !deadline.is_some_and(|d| d <= today);
        let mut tags = task.tags.clone();
        let (mut deferred, mut under_done) = (own_deferred, false);
        if let Some(p) = task.parent_id.as_ref().and_then(|p| self.index.get(p)).copied() {
            self.resolve(p, today, done);
            for tag in &self.tags[p] {
                if !tags.contains(tag) {
                    tags.push(tag.clone());
                }
            }
            deferred |= self.deferred[p];
            under_done = self.under_done[p] || self.tasks[p].completed_at.is_some();
        }
        self.tags[i] = tags;
        self.deferred[i] = deferred;
        self.under_done[i] = under_done;
    }

    fn candidate(&self, i: usize) -> Candidate<'_> {
        let t = &self.tasks[i];
        let date = |d: &Option<String>| d.as_deref().and_then(|d| Date::parse(d).ok());
        Candidate {
            title: &t.title,
            notes: &t.notes,
            start: date(&t.start),
            deadline: date(&t.deadline),
            someday: t.someday,
            priority: t.priority,
            repeating: t.repeat.is_some(),
            tags: &self.tags[i],
            deferred: self.deferred[i],
            done: t.completed_at.is_some(),
        }
    }

    fn has_ancestor_in(&self, i: usize, set: &HashSet<usize>) -> bool {
        let mut current = i;
        while let Some(p) = self.tasks[current]
            .parent_id
            .as_ref()
            .and_then(|p| self.index.get(p))
        {
            if set.contains(p) {
                return true;
            }
            current = *p;
        }
        false
    }

    /// Open tasks the query shows at the top level: matching, not under a completed task, and
    /// not under another match (they show in its subtree).
    fn open_tops(&self, query: &Query, today: Date) -> Vec<usize> {
        let matched: HashSet<usize> = (0..self.tasks.len())
            .filter(|&i| {
                self.tasks[i].completed_at.is_none()
                    && !self.under_done[i]
                    && query.matches(&self.candidate(i), today)
            })
            .collect();
        let mut tops: Vec<usize> = matched
            .iter()
            .copied()
            .filter(|&i| !self.has_ancestor_in(i, &matched))
            .collect();
        tops.sort_unstable();
        tops
    }
}

impl Store {
    // ---- views ----------------------------------------------------------------------------

    /// The built-in views, then saved ones in their order.
    pub fn views(&self, today: Date) -> Result<Vec<View>> {
        let ctx = Context::new(self.all_tasks()?, today);
        let mut out = Vec::new();
        for (id, title, query) in BUILT_IN {
            let q = Query::parse(query)?;
            out.push(View {
                id: id.into(),
                title: title.into(),
                query: String::new(),
                built_in: true,
                sort: self.view_sort(id),
                open_count: ctx.open_tops(&q, today).len() as u32,
            });
        }
        for (id, title, query) in self.saved_views() {
            let count = Query::parse(&query).map_or(0, |q| ctx.open_tops(&q, today).len() as u32);
            out.push(View {
                sort: self.view_sort(&id),
                id,
                title,
                query,
                built_in: false,
                open_count: count,
            });
        }
        Ok(out)
    }

    /// Saves a view. The query must parse (see [`crate::filter`]).
    pub fn create_view(&self, title: &str, query: &str) -> Result<String> {
        let title = crate::store::clean_title(title)?;
        Query::parse(query)?;
        let position = self
            .saved_views_raw()
            .iter()
            .map(|(_, m)| position_of(m))
            .fold(-1.0, f64::max)
            + 1.0;
        // This peer's op counter: unique per peer, as every view adds ops.
        let peer = self.doc.peer_id();
        let id = format!("v{peer}-{}", self.doc.oplog_vv().get(&peer).copied().unwrap_or(0));
        self.transact(|| {
            let view = self.views.ensure_mergeable_map(&id)?;
            view.insert(TITLE, title)?;
            view.insert(QUERY, query.trim())?;
            view.insert(POSITION, position)?;
            Ok(id.clone())
        })
    }

    pub fn update_view(&self, id: &str, title: &str, query: &str) -> Result<()> {
        let title = crate::store::clean_title(title)?;
        Query::parse(query)?;
        let view = self.saved_view(id)?;
        self.transact(|| {
            view.insert(TITLE, title)?;
            view.insert(QUERY, query.trim())?;
            Ok(())
        })
    }

    /// Deletes a saved view (its tasks stay). Undoable.
    pub fn delete_view(&self, id: &str) -> Result<()> {
        let view = self.saved_view(id)?;
        self.transact(|| Ok(view.insert(DELETED_AT, now_millis())?))
    }

    /// Moves a saved view to `index` among saved views.
    pub fn move_view(&self, id: &str, index: usize) -> Result<()> {
        self.saved_view(id)?;
        let mut ids: Vec<String> = self.saved_views().into_iter().map(|(id, ..)| id).collect();
        ids.retain(|v| v != id);
        ids.insert(index.min(ids.len()), id.to_owned());
        let maps: Vec<LoroMap> = ids.iter().map(|v| self.saved_view(v)).collect::<Result<_>>()?;
        self.transact(|| {
            for (i, view) in maps.iter().enumerate() {
                view.insert(POSITION, i as f64)?;
            }
            Ok(())
        })
    }

    /// How a view sorts its open tasks: as set, else Due for Today and Upcoming (always Due) and
    /// Manual for the rest.
    pub fn view_sort(&self, id: &str) -> TaskSort {
        if id == UPCOMING {
            return TaskSort::Due;
        }
        match self.task_sort.get(id) {
            Some(ValueOrContainer::Value(v)) => {
                v.as_string().map_or(TaskSort::Manual, |s| TaskSort::from_key(s))
            }
            _ if id == TODAY => TaskSort::Due,
            _ => TaskSort::Manual,
        }
    }

    pub fn set_view_sort(&self, id: &str, sort: TaskSort) -> Result<()> {
        self.view_query(id)?;
        if id == UPCOMING {
            return Err(Error::Invalid("Upcoming is always by date".into()));
        }
        self.transact(|| Ok(self.task_sort.insert(id, sort.key())?))
    }

    /// Whether `query` is a valid filter; the error says what's wrong.
    pub fn check_query(&self, query: &str) -> Result<()> {
        Query::parse(query).map(|_| ())
    }

    /// How many open tasks `query` shows at the top level, for previewing a view being edited.
    pub fn count_query(&self, query: &str, today: Date) -> Result<u32> {
        let q = Query::parse(query)?;
        Ok(Context::new(self.all_tasks()?, today).open_tops(&q, today).len() as u32)
    }

    /// A view's tasks arranged for display. See [`Outline`].
    pub fn view_outline(&self, id: &str, today: Date) -> Result<Outline> {
        self.view_outline_at(id, today, now_millis())
    }

    /// [`Store::view_outline`] as of `now` (Unix millis).
    pub fn view_outline_at(&self, id: &str, today: Date, now: i64) -> Result<Outline> {
        let query = self.view_query(id)?;
        let sort = self.view_sort(id);
        let ctx = Context::new(self.all_tasks()?, today);
        let open = self.ordered_tops(id, &ctx, &query, sort, today);
        let open_set: HashSet<usize> = open.iter().copied().collect();
        let mut completed: Vec<usize> = (0..ctx.tasks.len())
            .filter(|&i| {
                let t = &ctx.tasks[i];
                t.completed_at
                    .is_some_and(|at| query.shows_done() || at > now - ARCHIVE_AFTER_MS)
                    && !ctx.under_done[i]
                    && !ctx.has_ancestor_in(i, &open_set)
                    && query.matches_done(&ctx.candidate(i), today)
            })
            .collect();
        completed.sort_by_key(|&i| std::cmp::Reverse(ctx.tasks[i].completed_at));

        // Subtasks of each shown task, each level in tree order (sorted like the view).
        let mut children: HashMap<Option<String>, Vec<Task>> = HashMap::new();
        for task in &ctx.tasks {
            if task.parent_id.is_some() {
                children
                    .entry(task.parent_id.clone())
                    .or_default()
                    .push(task.clone());
            }
        }
        if sort != TaskSort::Manual {
            for kids in children.values_mut() {
                kids.sort_by(|a, b| sort.compare(a, b));
            }
        }
        let mut outline = Outline::default();
        for i in open {
            push_subtree(&mut outline.open, &mut children, ctx.tasks[i].clone(), 0, false);
        }
        for i in completed {
            push_subtree(
                &mut outline.completed,
                &mut children,
                ctx.tasks[i].clone(),
                0,
                true,
            );
        }
        Ok(outline)
    }

    /// Open tasks a view leaves out only because they start on a later day: what it will show once
    /// that day comes, soonest first. A subtask goes with its parent. Empty for views that already
    /// show such tasks (Today, Upcoming, Someday, and queries about dates).
    pub fn view_later(&self, id: &str, today: Date) -> Result<Vec<Task>> {
        let query = self.view_query(id)?;
        let ctx = Context::new(self.all_tasks()?, today);
        // Its start day, or for a subtask held back by its parent, the parent's.
        let start = |i: usize| {
            let mut at = i;
            loop {
                let t = &ctx.tasks[at];
                if let Some(s) = t
                    .start
                    .as_deref()
                    .and_then(|d| Date::parse(d).ok())
                    .filter(|s| *s > today)
                {
                    return Some(s);
                }
                at = *t.parent_id.as_ref().and_then(|p| ctx.index.get(p))?;
            }
        };
        let later: HashSet<usize> = (0..ctx.tasks.len())
            .filter(|&i| {
                let t = &ctx.tasks[i];
                let candidate = ctx.candidate(i);
                t.completed_at.is_none()
                    && !ctx.under_done[i]
                    && ctx.deferred[i]
                    // Someday isn't "later": it has no day to come.
                    && !t.someday
                    && start(i).is_some()
                    && !query.matches(&candidate, today)
                    && query.matches_done(&candidate, today)
            })
            .collect();
        let mut tops: Vec<usize> = later
            .iter()
            .copied()
            .filter(|&i| !ctx.has_ancestor_in(i, &later))
            .collect();
        tops.sort_by(|&a, &b| {
            start(a)
                .cmp(&start(b))
                .then_with(|| ctx.tasks[a].title.cmp(&ctx.tasks[b].title))
        });
        Ok(tops.into_iter().map(|i| ctx.tasks[i].clone()).collect())
    }

    /// What a task added in a view starts with, so it shows there: its tags, priority, start day
    /// or Someday.
    pub fn view_defaults(&self, id: &str, today: Date) -> Result<crate::filter::Defaults> {
        Ok(self.view_query(id)?.defaults(today))
    }

    /// Moves a task shown at the top level of a view right before `before` (another one shown
    /// there), or to the end when `None`. A sorted view switches to Manual, starting from the order
    /// shown, in the same undo step.
    pub fn move_in_view(&self, view: &str, id: &str, before: Option<&str>, today: Date) -> Result<()> {
        let query = self.view_query(view)?;
        let sort = self.view_sort(view);
        if view == UPCOMING {
            return Err(Error::Invalid("Upcoming is always by date".into()));
        }
        let ctx = Context::new(self.all_tasks()?, today);
        let mut shown: Vec<String> = self
            .ordered_tops(view, &ctx, &query, sort, today)
            .into_iter()
            .map(|i| ctx.tasks[i].id.clone())
            .collect();
        let from = shown
            .iter()
            .position(|t| t == id)
            .ok_or_else(|| Error::Invalid(format!("{id} isn't shown in {view}")))?;
        let moved = shown.remove(from);
        let to = match before {
            Some(b) => shown
                .iter()
                .position(|t| t == b)
                .ok_or_else(|| Error::Invalid(format!("{b} isn't shown in {view}")))?,
            None => shown.len(),
        };
        shown.insert(to, moved);
        // Subtasks shown sorted keep that order too (in the tree, which every view shares).
        let mut frozen: Vec<(String, Vec<String>)> = Vec::new();
        if sort != TaskSort::Manual {
            let mut kids: HashMap<&str, Vec<&Task>> = HashMap::new();
            for t in ctx.tasks.iter().filter(|t| t.completed_at.is_none()) {
                if let Some(p) = &t.parent_id {
                    kids.entry(p.as_str()).or_default().push(t);
                }
            }
            for (parent, mut list) in kids {
                list.sort_by(|a, b| sort.compare(a, b));
                frozen.push((parent.to_owned(), list.iter().map(|t| t.id.clone()).collect()));
            }
        }
        self.transact(|| {
            let order = self.view_order.ensure_mergeable_map(view)?;
            for (i, task) in shown.iter().enumerate() {
                order.insert(task, i as f64)?;
            }
            if sort != TaskSort::Manual {
                self.task_sort.insert(view, TaskSort::Manual.key())?;
                for (parent, kids) in &frozen {
                    let parent = TreeParentId::Node(tree_id(parent)?);
                    // Each to the front, last first, so they end up first in the order shown.
                    for kid in kids.iter().rev() {
                        self.tasks.mov_to(tree_id(kid)?, parent, 0)?;
                    }
                }
            }
            Ok(())
        })
    }

    /// Indents a task in a view: one shown at the top level becomes the last subtask of the open
    /// task shown right above it; a subtask indents as in [`Store::indent_task`]. Returns false when
    /// there is nothing above.
    pub fn indent_in_view(&self, view: &str, id: &str, today: Date) -> Result<bool> {
        let query = self.view_query(view)?;
        let ctx = Context::new(self.all_tasks()?, today);
        let tops = self.ordered_tops(view, &ctx, &query, self.view_sort(view), today);
        let Some(at) = tops.iter().position(|&i| ctx.tasks[i].id == id) else {
            return self.indent_task(id);
        };
        let Some(&above) = at.checked_sub(1).map(|a| &tops[a]) else {
            return Ok(false);
        };
        let (node, parent) = (self.node(id, &[KIND_TASK])?, tree_id(&ctx.tasks[above].id)?);
        self.write(|tree| Ok(tree.mov(node, TreeParentId::Node(parent))?))?;
        Ok(true)
    }

    /// Outdents a subtask as [`Store::outdent_task`] does, and places it right after its parent
    /// in `view`.
    pub fn outdent_in_view(&self, view: &str, id: &str) -> Result<bool> {
        self.outdent(id, Some(view))
    }

    /// Makes a subtask a sibling of its parent, right after it, keeping the tags it had through
    /// its ancestors (so a top-level task stays in the views it was in); in `view`'s order right
    /// after the parent. One undo step. False when it is top-level already.
    /// Moves a task under `parent` (another task) right before its child `before`, or last; or with
    /// no parent to the top level, shown in `view` right before `before` (a task shown at its top
    /// level), or last. One undo step. Tags it had only through the tasks it leaves are kept on it,
    /// so it stays in the lists it was in. For dragging a row sideways into or out of another.
    pub fn nest_in_view(
        &self,
        view: &str,
        id: &str,
        parent: Option<&str>,
        before: Option<&str>,
        today: Date,
    ) -> Result<()> {
        let node = self.node(id, &[KIND_TASK])?;
        let parent_node = parent.map(|p| self.node(p, &[KIND_TASK])).transpose()?;
        if let Some(p) = parent_node
            && (p == node || self.is_ancestor(node, p))
        {
            return Err(Error::Invalid("a task can't go under itself".into()));
        }
        let before_node = before.map(|b| self.node(b, &[KIND_TASK])).transpose()?;
        let task = self.task(id)?;
        let old = self.inherited_tags(task.parent_id.as_deref())?;
        let new = self.inherited_tags(parent)?;
        let mut tags = task.tags.clone();
        for tag in old.into_iter().filter(|t| !new.contains(t)) {
            if !tags.contains(&tag) {
                tags.push(tag);
            }
        }
        let retag = tags != task.tags;
        // At the top level, its place among the rows the view shows (manual order only).
        let order = if parent.is_none() && self.view_sort(view) == TaskSort::Manual && view != UPCOMING {
            let query = self.view_query(view)?;
            let ctx = Context::new(self.all_tasks()?, today);
            let mut shown: Vec<String> = self
                .ordered_tops(view, &ctx, &query, TaskSort::Manual, today)
                .into_iter()
                .map(|i| ctx.tasks[i].id.clone())
                .filter(|t| t != id)
                .collect();
            let at = before
                .and_then(|b| shown.iter().position(|t| t == b))
                .unwrap_or(shown.len());
            shown.insert(at, id.to_owned());
            Some(shown)
        } else {
            None
        };
        self.transact(|| {
            match (parent_node, before_node) {
                (Some(_), Some(b)) if b != node => self.tasks.mov_before(node, b)?,
                (Some(p), _) => self.tasks.mov(node, TreeParentId::Node(p))?,
                (None, _) => self.tasks.mov(node, TreeParentId::Root)?,
            }
            if retag {
                write_tags(&self.tasks.get_meta(node)?, &tags)?;
            }
            if let Some(shown) = &order {
                let map = self.view_order.ensure_mergeable_map(view)?;
                for (i, task) in shown.iter().enumerate() {
                    map.insert(task, i as f64)?;
                }
            }
            Ok(())
        })
    }

    /// Tags a task under `parent` gets through it and its ancestors.
    fn inherited_tags(&self, parent: Option<&str>) -> Result<Vec<String>> {
        let mut tags: Vec<String> = Vec::new();
        let mut at = parent.map(str::to_owned);
        while let Some(id) = at {
            let task = self.task(&id)?;
            for tag in task.tags {
                if !tags.contains(&tag) {
                    tags.push(tag);
                }
            }
            at = task.parent_id;
        }
        Ok(tags)
    }

    pub(crate) fn outdent(&self, id: &str, view: Option<&str>) -> Result<bool> {
        let node = self.node(id, &[KIND_TASK])?;
        let Some(TreeParentId::Node(parent)) = self.tasks.parent(node) else {
            return Ok(false);
        };
        if projection::kind(&self.tasks, parent).as_deref() != Some(KIND_TASK) {
            return Ok(false);
        }
        let to_top = !matches!(self.tasks.parent(parent), Some(TreeParentId::Node(_)));
        let mut tags = self.task(id)?.tags;
        if to_top {
            let mut above = self.task(&parent.to_string())?;
            loop {
                for tag in above.tags {
                    if !tags.contains(&tag) {
                        tags.push(tag);
                    }
                }
                match above.parent_id {
                    Some(p) => above = self.task(&p)?,
                    None => break,
                }
            }
        }
        let place = view.and_then(|v| {
            let order = self.order_keys(v);
            let after = order.get(&parent.to_string()).copied()?;
            let next = order.values().copied().filter(|&k| k > after).reduce(f64::min);
            Some((v.to_owned(), next.map_or(after + 1.0, |n| (after + n) / 2.0)))
        });
        self.write(|tree| {
            tree.mov_after(node, parent)?;
            if to_top {
                write_tags(&tree.get_meta(node)?, &tags)?;
            }
            if let Some((v, key)) = &place {
                self.view_order.ensure_mergeable_map(v)?.insert(id, *key)?;
            }
            Ok(())
        })?;
        Ok(true)
    }

    /// Which top-level parent and defaults a new task gets in `target`: a task id makes a subtask
    /// (no defaults), anything else is a view.
    pub(crate) fn target(&self, target: &str, today: Date) -> Result<(TreeParentId, Option<Defaults>)> {
        if let Ok(node) = self.node(target, &[KIND_TASK]) {
            return Ok((TreeParentId::Node(node), None));
        }
        let query = self.view_query(target)?;
        Ok((TreeParentId::Root, Some(query.defaults(today))))
    }

    /// The query a view id (or an ad-hoc `#tag` query) stands for.
    pub(crate) fn view_query(&self, id: &str) -> Result<Query> {
        if let Some((.., q)) = BUILT_IN.iter().find(|(v, ..)| *v == id) {
            return Query::parse(q);
        }
        if id.starts_with('#') {
            return Query::parse(id);
        }
        let view = self.saved_view(id)?;
        Query::parse(&meta_str(&view, QUERY).unwrap_or_default())
    }

    /// Open top-level rows of a view in display order. Manual: tasks never placed in this view
    /// first (newest first), then the placed ones in their order. Other sorts break ties that way.
    fn ordered_tops(
        &self,
        view: &str,
        ctx: &Context,
        query: &Query,
        sort: TaskSort,
        today: Date,
    ) -> Vec<usize> {
        let order = self.order_keys(view);
        let mut tops = ctx.open_tops(query, today);
        let manual = |i: &usize| {
            let t = &ctx.tasks[*i];
            let key = order.get(&t.id).copied();
            // Unplaced first; among them, tree order (new top-level tasks are created first).
            (key.is_some(), key.unwrap_or(0.0), t.parent_id.is_some(), *i)
        };
        tops.sort_by(|a, b| {
            sort.compare(&ctx.tasks[*a], &ctx.tasks[*b]).then_with(|| {
                manual(a)
                    .partial_cmp(&manual(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        });
        tops
    }

    fn order_keys(&self, view: &str) -> HashMap<String, f64> {
        let mut keys = HashMap::new();
        if let Some(ValueOrContainer::Container(loro::Container::Map(map))) = self.view_order.get(view) {
            map.for_each(|task, value| {
                if let ValueOrContainer::Value(LoroValue::Double(k)) = value {
                    keys.insert(task.to_owned(), k);
                }
            });
        }
        keys
    }

    fn saved_view(&self, id: &str) -> Result<LoroMap> {
        match self.views.get(id) {
            Some(ValueOrContainer::Container(loro::Container::Map(m))) if m.get(DELETED_AT).is_none() => {
                Ok(m)
            }
            _ => Err(Error::NotFound(id.to_owned())),
        }
    }

    fn saved_views_raw(&self) -> Vec<(String, LoroMap)> {
        let mut out = Vec::new();
        self.views.for_each(|id, value| {
            if let ValueOrContainer::Container(loro::Container::Map(m)) = value
                && m.get(DELETED_AT).is_none()
            {
                out.push((id.to_owned(), m));
            }
        });
        out
    }

    /// (id, title, query) of saved views in order.
    fn saved_views(&self) -> Vec<(String, String, String)> {
        let mut views = self.saved_views_raw();
        views.sort_by(|(a, x), (b, y)| {
            position_of(x)
                .partial_cmp(&position_of(y))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.cmp(b))
        });
        views
            .into_iter()
            .map(|(id, m)| {
                let title = meta_str(&m, TITLE).unwrap_or_default();
                let query = meta_str(&m, QUERY).unwrap_or_default();
                (id, title, query)
            })
            .collect()
    }

    // ---- tags -----------------------------------------------------------------------------

    /// Every tag on a task, by name, with how many open tasks carry it.
    pub fn tags(&self) -> Result<Vec<TagInfo>> {
        let mut counts: HashMap<String, u32> = HashMap::new();
        for task in self.all_tasks()? {
            for tag in task.tags {
                *counts.entry(tag).or_default() += u32::from(task.completed_at.is_none());
            }
        }
        let mut tags: Vec<TagInfo> = counts
            .into_iter()
            .map(|(name, open_count)| TagInfo { name, open_count })
            .collect();
        tags.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(tags)
    }

    /// Renames a tag on every task and in saved views' queries, in one undo step.
    pub fn rename_tag(&self, old: &str, new: &str) -> Result<()> {
        let (old, new) = (normalize_tag(old), normalize_tag(new));
        if new.is_empty() {
            return Err(Error::Invalid("the new tag is empty".into()));
        }
        self.retag(&old, Some(&new))
    }

    /// Takes a tag off every task, in one undo step. Saved views naming it are left as they are.
    pub fn delete_tag(&self, tag: &str) -> Result<()> {
        self.retag(&normalize_tag(tag), None)
    }

    fn retag(&self, old: &str, new: Option<&str>) -> Result<()> {
        let tasks: Vec<Task> = self
            .all_tasks()?
            .into_iter()
            .filter(|t| t.tags.iter().any(|x| x == old))
            .collect();
        let views = self.saved_views_raw();
        self.write(|tree| {
            for task in &tasks {
                let mut tags: Vec<String> = task.tags.iter().filter(|t| *t != old).cloned().collect();
                if let Some(new) = new
                    && !tags.iter().any(|t| t == new)
                {
                    tags.push(new.to_owned());
                }
                write_tags(&tree.get_meta(tree_id(&task.id)?)?, &tags)?;
            }
            if let Some(new) = new {
                for (_, view) in &views {
                    if let Some(q) = meta_str(view, QUERY).and_then(|q| rename_tag_in(&q, old, new)) {
                        view.insert(QUERY, q)?;
                    }
                }
            }
            Ok(())
        })
    }

    // ---- migration ------------------------------------------------------------------------

    /// Turns lists from before ADR 0014 into tags and saved views: each live list's tasks get the
    /// tag `slug(title)` and move to the top level, a saved view with the list's id shows `#slug`
    /// in the list's order and sort, and the list node is soft-deleted. Ids and keys all derive
    /// from the list, so replicas doing this concurrently converge. Not undoable.
    pub(crate) fn migrate_lists(&self) -> Result<()> {
        let tree = &self.tasks;
        let lists: Vec<_> = tree
            .children(TreeParentId::Root)
            .unwrap_or_default()
            .into_iter()
            .filter(|&n| {
                projection::kind(tree, n).as_deref() == Some(KIND_LIST)
                    && tree.get_meta(n).is_ok_and(|m| m.get(DELETED_AT).is_none())
            })
            .collect();
        if lists.is_empty() {
            return Ok(());
        }
        let first_position = self
            .saved_views_raw()
            .iter()
            .map(|(_, m)| position_of(m))
            .fold(-1.0, f64::max)
            + 1.0;
        self.transact_with_origin(MIGRATE_ORIGIN, || {
            for (i, &list) in lists.iter().enumerate() {
                let meta = tree.get_meta(list)?;
                let title = meta_str(&meta, TITLE).unwrap_or_default();
                let tag = match normalize_tag(&title) {
                    t if t.is_empty() => "list".to_owned(),
                    t => t,
                };
                let id = list.to_string();
                let order = self.view_order.ensure_mergeable_map(&id)?;
                let mut placed = 0;
                for child in tree.children(list).unwrap_or_default() {
                    let child_meta = tree.get_meta(child)?;
                    child_meta.ensure_mergeable_map(TAGS)?.insert(&tag, true)?;
                    if child_meta.get(DELETED_AT).is_none()
                        && meta_i64(&child_meta, projection::COMPLETED_AT).is_none()
                    {
                        order.insert(&child.to_string(), f64::from(placed))?;
                        placed += 1;
                    }
                    tree.mov(child, TreeParentId::Root)?;
                }
                let view = self.views.ensure_mergeable_map(&id)?;
                view.insert(TITLE, title)?;
                view.insert(QUERY, format!("#{tag}"))?;
                view.insert(POSITION, first_position + i as f64)?;
                meta.insert(DELETED_AT, now_millis())?;
            }
            Ok(())
        })
    }
}

fn position_of(view: &LoroMap) -> f64 {
    match view.get(POSITION) {
        Some(ValueOrContainer::Value(LoroValue::Double(p))) => p,
        _ => 0.0,
    }
}
