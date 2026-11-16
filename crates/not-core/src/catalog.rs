//! The exercise catalogue: wger's public exercise database (English and French names, muscles,
//! equipment), bundled at build time from `data/exercises.json` (see `scripts/fetch-exercises.sh`).
//!
//! The user's own exercise names stay as typed. A name is linked to a catalogue exercise
//! automatically when it matches one of its names or aliases (ignoring case and accents), or by
//! hand through `exercise_links` in the workspace doc (name key → catalogue id, `""` to unlink).
//! Names linked to the same exercise share history and the "last time" hint.

use std::collections::HashMap;
use std::sync::OnceLock;

use loro::{LoroValue, ValueOrContainer};
use serde::Deserialize;

use crate::Result;
use crate::store::Store;

static DATA: &str = include_str!("../data/exercises.json");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CatalogExercise {
    /// wger's exercise uuid.
    pub id: String,
    #[serde(rename = "en")]
    pub name_en: String,
    #[serde(rename = "fr")]
    pub name_fr: Option<String>,
    pub aliases: Vec<String>,
    /// Abs, Arms, Back, Calves, Cardio, Chest, Legs or Shoulders.
    pub category: String,
    pub muscles: Vec<String>,
    #[serde(rename = "secondary")]
    pub secondary_muscles: Vec<String>,
    pub equipment: Vec<String>,
    /// `CC-BY-SA 4`, `CC-BY-SA 3` or `CC0`.
    pub license: String,
    /// wger username of the author; may be empty.
    pub author: String,
}

/// Working sets that trained a muscle (as a primary muscle) over some days.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MuscleSets {
    pub muscle: String,
    pub sets: u32,
}

struct Catalog {
    items: Vec<CatalogExercise>,
    by_id: HashMap<String, usize>,
    /// Every name and alias, normalized.
    by_name: HashMap<String, usize>,
    /// Normalized names and aliases per item, for search.
    names: Vec<Vec<String>>,
}

fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let items: Vec<CatalogExercise> =
            serde_json::from_str(DATA).expect("bundled exercises.json is valid");
        let names: Vec<Vec<String>> = items
            .iter()
            .map(|e| {
                std::iter::once(&e.name_en)
                    .chain(e.name_fr.as_ref())
                    .chain(&e.aliases)
                    .map(|n| normalize(n))
                    .collect()
            })
            .collect();
        let mut by_name = HashMap::new();
        for (i, list) in names.iter().enumerate() {
            for n in list {
                by_name.entry(n.clone()).or_insert(i);
            }
        }
        let by_id = items.iter().enumerate().map(|(i, e)| (e.id.clone(), i)).collect();
        Catalog {
            items,
            by_id,
            by_name,
            names,
        }
    })
}

/// Lowercase, without accents, punctuation as spaces, single spaces.
pub(crate) fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        match c {
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => out.push('a'),
            'é' | 'è' | 'ê' | 'ë' => out.push('e'),
            'î' | 'ï' | 'í' | 'ì' => out.push('i'),
            'ô' | 'ö' | 'ó' | 'ò' | 'õ' => out.push('o'),
            'ù' | 'û' | 'ü' | 'ú' => out.push('u'),
            'ç' => out.push('c'),
            'ñ' => out.push('n'),
            'œ' => out.push_str("oe"),
            'æ' => out.push_str("ae"),
            'ß' => out.push_str("ss"),
            c if c.is_alphanumeric() => out.push(c),
            _ => out.push(' '),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// How well `query` matches `name` (both normalized); 0 for no match.
fn score(query: &str, words: &[&str], name: &str) -> u32 {
    if name == query {
        return 100;
    }
    if name.starts_with(query) {
        return 80;
    }
    let name_words: Vec<&str> = name.split(' ').collect();
    if words.iter().all(|w| name_words.iter().any(|n| n.starts_with(w))) {
        return 60;
    }
    if name.contains(query) {
        return 40;
    }
    0
}

/// Catalogue exercises matching `query` in English, French or an alias, best first.
pub fn search(query: &str, limit: usize) -> Vec<CatalogExercise> {
    let query = normalize(query);
    if query.is_empty() {
        return Vec::new();
    }
    let words: Vec<&str> = query.split(' ').collect();
    let cat = catalog();
    let mut hits: Vec<(u32, usize, usize)> = cat
        .names
        .iter()
        .enumerate()
        .filter_map(|(i, names)| {
            let best = names.iter().map(|n| score(&query, &words, n)).max().unwrap_or(0);
            (best > 0).then(|| (best, names[0].len(), i))
        })
        .collect();
    hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    hits.into_iter()
        .take(limit)
        .map(|(_, _, i)| cat.items[i].clone())
        .collect()
}

pub fn by_id(id: &str) -> Option<CatalogExercise> {
    let cat = catalog();
    cat.by_id.get(id).map(|&i| cat.items[i].clone())
}

/// The catalogue exercise a name matches exactly (any language or alias).
fn by_name(name: &str) -> Option<&'static CatalogExercise> {
    let cat = catalog();
    cat.by_name.get(&normalize(name)).map(|&i| &cat.items[i])
}

impl Store {
    /// Catalogue exercises matching `query`, best first.
    pub fn search_exercises(&self, query: &str, limit: usize) -> Vec<CatalogExercise> {
        search(query, limit)
    }

    /// What a name of the user's is linked to: by hand, else by matching a catalogue name.
    pub fn exercise_info(&self, name: &str) -> Option<CatalogExercise> {
        self.resolve(&crate::workouts::name_key(name))
            .and_then(|id| by_id(&id))
    }

    /// Links the user's name to a catalogue exercise (`None` unlinks, overriding a name match).
    pub fn link_exercise(&self, name: &str, catalog_id: Option<&str>) -> Result<()> {
        if let Some(id) = catalog_id
            && by_id(id).is_none()
        {
            return Err(crate::Error::NotFound(id.to_owned()));
        }
        let key = crate::workouts::name_key(name);
        self.transact(|| Ok(self.exercise_links.insert(&key, catalog_id.unwrap_or(""))?))
    }

    /// Catalogue id for a name key, if linked.
    pub(crate) fn resolve(&self, key: &str) -> Option<String> {
        match self.exercise_links.get(key) {
            Some(ValueOrContainer::Value(LoroValue::String(id))) if id.is_empty() => None,
            Some(ValueOrContainer::Value(LoroValue::String(id))) => Some(id.to_string()),
            _ => by_name(key).map(|e| e.id.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_catalogue_loads() {
        assert!(catalog().items.len() > 500);
        assert!(catalog().items.iter().all(|e| !e.name_en.is_empty()));
    }

    #[test]
    fn search_matches_french_without_accents() {
        let hits = search("developpe couche halteres", 5);
        assert!(
            hits.iter()
                .any(|e| e.name_fr.as_deref() == Some("Développé couché avec haltères")),
            "{hits:#?}"
        );
        assert_eq!(search("Bench Press", 1)[0].name_en, "Bench Press");
        assert!(search("", 5).is_empty());
    }

    #[test]
    fn normalizes() {
        assert_eq!(
            normalize("  Curl Biceps Haltères (double) "),
            "curl biceps halteres double"
        );
        assert_eq!(normalize("Élévations latérales"), "elevations laterales");
    }
}
