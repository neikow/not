# 0010 — Images in notes live beside the doc, by content hash

Status: accepted (2026-11-12)

## Context
Notes need pictures (photos, screenshots). The workspace is one Loro doc whose history every device
keeps for good (0001, 0008); putting image bytes in it would grow every device's doc, snapshot and
sync forever, with no way to collect deleted pictures.

## Decision
- **A note points at an image with a markdown line of its own:** `![](not-image:<id>)`. The text stays
  plain markdown; editors hide the markup and draw the picture over the line, like checkboxes.
- **The id is the SHA-256 of the bytes plus the kind's extension** (`<64 hex>.jpg|png|webp|gif`, see
  `not_protocol::image_id`). The same picture is stored once, and anyone can check bytes against
  their name.
- **Bytes live in a `blob` table beside the doc** on each device and on the server, not in Loro.
- **Sync moves them after the text:** upload what this device added (`PUT /v1/blobs/{id}`, which the
  server refuses unless the bytes hash to the id), then download what notes point at and the device
  lacks (`GET /v1/blobs/{id}`). Image trouble never fails text sync; it is retried next round.
- **The apps downscale before storing:** longest side 2048 px, JPEG at 85 % (PNG when there is
  transparency), EXIF rotation applied. A photo is a few hundred KB.
- The owner's web pages show images at `/img/{id}`; shared list links never do.

## Consequences
- A note can show before its pictures arrive; the editors draw a placeholder until then.
- Deleting a note leaves its blobs behind (no collection yet). Cheap for a single user; a sweep of
  unreferenced blobs can come later.
- Backups (the Loro snapshot) don't include images yet.
