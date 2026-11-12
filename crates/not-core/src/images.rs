//! Images in notes. A note holds `![](not-image:<id>)` on a line of its own; the bytes live in the
//! `blob` table under that content id (SHA-256 + extension, see [`not_protocol::image_id`]), not
//! in the Loro doc, so the doc and its history stay small. Sync uploads blobs the server lacks and
//! downloads the ones notes point at. See `docs/adr/0010-images-in-notes.md`.

use std::collections::BTreeSet;

use not_protocol::{image_id, is_image_id};
use rusqlite::{OptionalExtension, params};

use crate::markdown;
use crate::store::{ChangeOrigin, Store};
use crate::{Error, Result};

#[cfg_attr(not(feature = "net"), allow(dead_code))]
impl Store {
    /// Keeps an image for notes and returns its id, to put in `![](not-image:<id>)`. The apps
    /// downscale and re-encode first; this takes the bytes as they are.
    pub fn add_image(&self, bytes: &[u8]) -> Result<String> {
        let id =
            image_id(bytes).ok_or_else(|| Error::Invalid("not a JPEG, PNG, WebP or GIF image".into()))?;
        self.lock().db.execute(
            "INSERT OR IGNORE INTO blob (id, data, uploaded) VALUES (?1, ?2, 0)",
            params![id, bytes],
        )?;
        Ok(id)
    }

    /// The image's bytes, or `None` while it hasn't arrived from the server yet.
    pub fn image(&self, id: &str) -> Result<Option<Vec<u8>>> {
        Ok(self
            .lock()
            .db
            .query_row("SELECT data FROM blob WHERE id = ?1", [id], |r| r.get(0))
            .optional()?)
    }

    /// Images added here that the server doesn't have yet.
    pub(crate) fn images_to_upload(&self) -> Result<Vec<String>> {
        let inner = self.lock();
        let mut stmt = inner
            .db
            .prepare_cached("SELECT id FROM blob WHERE uploaded = 0 ORDER BY id")?;
        Ok(stmt
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub(crate) fn image_uploaded(&self, id: &str) -> Result<()> {
        self.lock()
            .db
            .execute("UPDATE blob SET uploaded = 1 WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Ids notes point at that aren't here yet: what sync downloads.
    pub(crate) fn missing_images(&self) -> Result<Vec<String>> {
        let inner = self.lock();
        let mut stmt = inner
            .db
            .prepare_cached("SELECT body FROM note WHERE body LIKE '%not-image:%'")?;
        let mut wanted = BTreeSet::new();
        for body in stmt.query_map([], |r| r.get::<_, String>(0))? {
            wanted.extend(markdown::images(&body?).into_iter().map(|i| i.id));
        }
        let mut have = inner.db.prepare_cached("SELECT 1 FROM blob WHERE id = ?1")?;
        let mut missing = Vec::new();
        for id in wanted {
            if !have.exists([&id])? {
                missing.push(id);
            }
        }
        Ok(missing)
    }

    /// Keeps an image downloaded from the server, after checking it is what its id says, and
    /// tells listeners so notes showing a placeholder redraw.
    pub(crate) fn image_downloaded(&self, id: &str, bytes: &[u8]) -> Result<()> {
        if !is_image_id(id) || image_id(bytes).as_deref() != Some(id) {
            return Err(Error::Invalid(format!("image {id} doesn't match its contents")));
        }
        self.lock().db.execute(
            "INSERT OR IGNORE INTO blob (id, data, uploaded) VALUES (?1, ?2, 1)",
            params![id, bytes],
        )?;
        self.notify(ChangeOrigin::Remote);
        Ok(())
    }
}
