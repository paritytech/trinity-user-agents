package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

class Migration15To16 : Migration(15, 16) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL(
            """
            ALTER TABLE chat_messages ADD COLUMN isInternal INTEGER NOT NULL DEFAULT 0
            """.trimIndent()
        )

        db.execSQL(
            """
            UPDATE chat_messages SET isInternal = 1 WHERE type = 'TOKEN'
            """.trimIndent()
        )
    }
}
