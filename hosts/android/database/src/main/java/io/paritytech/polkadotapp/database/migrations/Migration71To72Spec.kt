package io.paritytech.polkadotapp.database.migrations

import androidx.room.DeleteTable
import androidx.room.migration.AutoMigrationSpec

@DeleteTable.Entries(
    DeleteTable(tableName = "pocket_cards"),
    DeleteTable(tableName = "pocket_card_faces"),
)
class Migration71To72Spec : AutoMigrationSpec
