//! A few tasks that show the basics, offered on a fresh install (first run → Start fresh): each
//! one says what it shows, and ticking them off is the first thing to try. Same words on every
//! platform, so they only name things every app has.

use loro::TreeParentId;

use crate::Result;
use crate::date::Date;
use crate::schedule::Schedule;
use crate::store::{Store, create_task_node, fill_task};

/// The tag on the task that shows tags; it makes a list of its own.
pub const STARTER_TAG: &str = "getting-started";

struct Starter {
    title: &'static str,
    /// Starts today, so it's in Today too.
    today: bool,
    tagged: bool,
    subtasks: &'static [&'static str],
}

/// Top to bottom, as they show in the Inbox.
const STARTERS: &[Starter] = &[
    Starter {
        title: "Welcome to not! Tick this off once you've read it",
        today: true,
        tagged: false,
        subtasks: &[],
    },
    Starter {
        title: "Add tasks the way you'd say them: “call mum fri 6pm #family !1”",
        today: false,
        tagged: false,
        subtasks: &[],
    },
    Starter {
        title: "Open a task for notes, subtasks, a deadline or a repeat",
        today: false,
        tagged: false,
        subtasks: &["Subtasks look like this", "Tick them off one by one"],
    },
    Starter {
        title: "Tags make lists: this one is under #getting-started, not in the Inbox",
        today: false,
        tagged: true,
        subtasks: &[],
    },
    Starter {
        title: "Changed your mind? Undo takes anything back",
        today: false,
        tagged: false,
        subtasks: &[],
    },
    Starter {
        title: "An idea, or something broken? Settings → About",
        today: false,
        tagged: false,
        subtasks: &[],
    },
];

impl Store {
    /// Adds the starter tasks (see the module docs) in one undo step. Returns how many top-level
    /// tasks it added.
    pub fn add_starter_tasks(&self, today: Date) -> Result<u32> {
        let tag = [STARTER_TAG.to_owned()];
        self.write(|tree| {
            // New top-level tasks go first, so add them bottom-up.
            for starter in STARTERS.iter().rev() {
                let schedule = Schedule {
                    start: starter.today.then_some(today),
                    ..Schedule::default()
                };
                let tags: &[String] = if starter.tagged { &tag } else { &[] };
                let id = create_task_node(tree, TreeParentId::Root)?;
                fill_task(&tree.get_meta(id)?, starter.title, &schedule, 0, tags)?;
                for sub in starter.subtasks {
                    let child = create_task_node(tree, TreeParentId::Node(id))?;
                    fill_task(&tree.get_meta(child)?, sub, &Schedule::default(), 0, &[])?;
                }
            }
            Ok(STARTERS.len() as u32)
        })
    }
}
