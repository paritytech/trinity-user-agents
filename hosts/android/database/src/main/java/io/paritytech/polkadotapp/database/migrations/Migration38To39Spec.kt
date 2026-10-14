package io.paritytech.polkadotapp.database.migrations

import androidx.room.DeleteColumn
import androidx.room.migration.AutoMigrationSpec

@DeleteColumn.Entries(
    DeleteColumn(tableName = "sso_sessions", columnName = "name"),
    DeleteColumn(tableName = "sso_sessions", columnName = "icon"),
    DeleteColumn(tableName = "sso_sessions", columnName = "hostVersion"),
    DeleteColumn(tableName = "sso_sessions", columnName = "platformType"),
    DeleteColumn(tableName = "sso_sessions", columnName = "platformVersion"),
)
class Migration38To39Spec : AutoMigrationSpec
