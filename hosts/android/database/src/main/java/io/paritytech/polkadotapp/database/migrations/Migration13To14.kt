package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

class Migration13To14 : Migration(13, 14) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE contacts ADD COLUMN origin TEXT NOT NULL DEFAULT 'contactChat'")
        db.execSQL("ALTER TABLE game_players ADD COLUMN gameTimestamp INTEGER")
        db.execSQL(
            """
            UPDATE contacts
            SET origin = 'sharedGame'
            WHERE accountId IN (SELECT DISTINCT accountId FROM game_players)
            """.trimIndent()
        )
    }
}
