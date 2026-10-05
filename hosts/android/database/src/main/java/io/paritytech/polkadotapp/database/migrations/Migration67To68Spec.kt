package io.paritytech.polkadotapp.database.migrations

import androidx.room.DeleteTable
import androidx.room.migration.AutoMigrationSpec

@DeleteTable.Entries(
    DeleteTable(tableName = "video_game_votes"),
    DeleteTable(tableName = "video_game_banned_players"),
    DeleteTable(tableName = "video_game_connection_attempts"),
    DeleteTable(tableName = "game_players"),
    DeleteTable(tableName = "vouchers"),
)
class Migration67To68Spec : AutoMigrationSpec
