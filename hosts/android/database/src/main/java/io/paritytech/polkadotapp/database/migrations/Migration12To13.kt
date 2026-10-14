package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

class Migration12To13 : Migration(12, 13) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("DROP TABLE IF EXISTS video_game_votes")
        db.execSQL(
            """
            CREATE TABLE IF NOT EXISTS `video_game_votes` (
                `id` INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                `accountId` BLOB NOT NULL,
                `roundIndex` INTEGER NOT NULL,
                `playerIndex` INTEGER NOT NULL,
                `isPerson` INTEGER NOT NULL,
                `gameIndex` INTEGER NOT NULL
            )
            """.trimIndent()
        )
    }
}
