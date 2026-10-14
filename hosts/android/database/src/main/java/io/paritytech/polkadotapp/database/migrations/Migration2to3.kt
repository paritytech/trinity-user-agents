package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

class Migration2to3 : Migration(2, 3) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL(
            """
            CREATE TABLE IF NOT EXISTS `video_game_votes` (
                `id` INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                `accountId` BLOB NOT NULL,
                `roundIndex` INTEGER NOT NULL,
                `playerIndex` INTEGER NOT NULL,
                `isPerson` INTEGER NOT NULL
            )
            """.trimIndent()
        )

        db.execSQL(
            """
            CREATE TABLE IF NOT EXISTS `vouchers` (
                `state` TEXT NOT NULL,
                `type` TEXT NOT NULL,
                `voucherIndex` INTEGER NOT NULL,
                `value` TEXT NOT NULL,
                PRIMARY KEY(`type`, `voucherIndex`)
            )
            """.trimIndent()
        )
    }
}
