package io.paritytech.polkadotapp.database.migrations

import androidx.room.DeleteColumn
import androidx.room.migration.AutoMigrationSpec

/**
 * Drops the legacy display columns from `sso_sessions` — name/icon now live in the new
 * `sso_session_metadata` table. Added-/sync-related columns (addedAt, status, lastUpdate,
 * outgoingUpdateTime) are handled automatically by Room.
 */
@DeleteColumn.Entries(
    DeleteColumn(tableName = "sso_sessions", columnName = "name"),
    DeleteColumn(tableName = "sso_sessions", columnName = "icon"),
)
class Migration42To43Spec : AutoMigrationSpec
