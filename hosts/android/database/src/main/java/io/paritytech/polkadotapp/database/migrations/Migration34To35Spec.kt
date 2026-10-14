package io.paritytech.polkadotapp.database.migrations

import androidx.room.RenameColumn
import androidx.room.migration.AutoMigrationSpec

@RenameColumn.Entries(
    RenameColumn(
        tableName = "file_uploads",
        fromColumnName = "originalFileSize",
        toColumnName = "fileSize"
    )
)
class Migration34To35Spec : AutoMigrationSpec
