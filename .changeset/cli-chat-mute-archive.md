---
"@parity/truapi": minor
---

The `truapi-host dev` chat page lists `mute_archive`: the room header mutes (1 hour, 8 hours, 1 week or always) and
archives a room, the list greys a muted room's badge and keeps archived rooms under an Archived row, each change reaches
the product as `UserStateChanged`, and a Notifications list shows every notification the host delivered. The core shows
nothing for an immediate notification into a muted room.
