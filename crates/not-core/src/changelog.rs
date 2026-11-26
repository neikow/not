//! What's new, in the apps' words: a few lines per release, newest first. The apps show the
//! releases since the version last opened, once after an update (never on a fresh install), and
//! all of them from About. Add an entry with each release tag; the full commit list stays in the
//! GitHub release notes.

/// One release's highlights.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: &'static str,
    pub highlights: &'static [&'static str],
}

/// Newest first.
pub const RELEASES: &[Release] = &[Release {
    version: "0.2.1",
    highlights: &[
        "Widgets update right away when you tick something on them.",
        "Repeating tasks with subtasks: each round is logged with its checklist, repeating subtasks no longer pile up, and dated subtasks move along.",
        "Steadier dragging and nesting of subtasks; in Upcoming, subtasks stay under their task's day.",
        "The web companion has Habits (check in from the browser), Workouts, search, tags and Upcoming by day.",
        "Bring your tasks from Todoist or TickTick: Settings → Import, export and backup.",
        "New “done” filter word: a smart list like “done #work” keeps everything you finished.",
        "A first run that starts fresh, with a few example tasks that show the basics.",
        "About: request a feature, report a bug, see what's new and the open-source licenses.",
        "Settings are simpler: the Archive is now the Logbook and “done” lists.",
    ],
}];

/// Releases newer than `since` and no newer than `current`, newest first. Nothing when `since`
/// is empty (a fresh install has nothing to catch up on) or a version doesn't parse.
pub fn whats_new(since: &str, current: &str) -> Vec<Release> {
    let (Some(since), Some(current)) = (parse(since), parse(current)) else {
        return Vec::new();
    };
    RELEASES
        .iter()
        .filter(|r| parse(r.version).is_some_and(|v| v > since && v <= current))
        .cloned()
        .collect()
}

/// "1.2.3" (or "v1.2.3") → (1, 2, 3).
fn parse(version: &str) -> Option<(u32, u32, u32)> {
    let mut parts = version
        .trim()
        .trim_start_matches('v')
        .split('.')
        .map(|p| p.parse().ok());
    let v = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn releases_are_newest_first_with_valid_versions() {
        let versions: Vec<_> = RELEASES.iter().map(|r| parse(r.version).unwrap()).collect();
        assert!(versions.windows(2).all(|w| w[0] > w[1]));
        assert!(RELEASES.iter().all(|r| !r.highlights.is_empty()));
    }

    #[test]
    fn only_what_came_since_the_last_version_opened() {
        assert_eq!(whats_new("0.2.0", "0.2.1").len(), 1);
        assert!(whats_new("0.2.1", "0.2.1").is_empty(), "already seen");
        assert!(whats_new("", "0.2.1").is_empty(), "fresh install");
        assert!(
            whats_new("0.1.0", "0.2.0").is_empty(),
            "not past the installed version"
        );
        assert_eq!(whats_new("v0.1.11", "v0.2.1").len(), 1);
        assert_eq!(parse("1.2"), None);
        assert_eq!(parse("1.2.3.4"), None);
    }
}
