package io.paritytech.polkadotapp.database.migrations

import androidx.room.DeleteColumn
import androidx.room.migration.AutoMigrationSpec

@DeleteColumn.Entries(
    DeleteColumn(
        tableName = "file_uploads",
        columnName = "text"
    )
)
class Migration32To33Spec : AutoMigrationSpec
