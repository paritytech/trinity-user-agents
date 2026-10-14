package io.paritytech.polkadotapp.database.migrations

import androidx.room.DeleteColumn
import androidx.room.migration.AutoMigrationSpec

// Room auto-detects the added columns; only the drops need declaring.
@DeleteColumn.Entries(
    DeleteColumn(tableName = "products", columnName = "scriptUrl"),
    DeleteColumn(tableName = "products", columnName = "contentHash"),
    DeleteColumn(tableName = "products", columnName = "iconUrl"),
)
class Migration54To55Spec : AutoMigrationSpec
