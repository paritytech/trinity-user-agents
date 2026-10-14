package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

class Migration14To15 : Migration(14, 15) {
    override fun migrate(db: SupportSQLiteDatabase) {
        createChatRequestsTable(db)
        createChatRequestSyncStateTable(db)
        addChatRequestIdToContacts(db)
    }

    private fun createChatRequestsTable(db: SupportSQLiteDatabase) {
        db.execSQL(
            """
            CREATE TABLE IF NOT EXISTS chat_requests (
                id TEXT NOT NULL PRIMARY KEY,
                timestamp INTEGER NOT NULL,
                direction TEXT NOT NULL,
                status TEXT NOT NULL
            )
            """.trimIndent()
        )
    }

    private fun createChatRequestSyncStateTable(db: SupportSQLiteDatabase) {
        db.execSQL(
            """
            CREATE TABLE IF NOT EXISTS chat_request_sync_state (
                metaAccountId INTEGER NOT NULL PRIMARY KEY,
                lastSyncedDay INTEGER
            )
            """.trimIndent()
        )
    }

    private fun addChatRequestIdToContacts(db: SupportSQLiteDatabase) {
        db.execSQL(
            """
            ALTER TABLE contacts ADD COLUMN chatRequestId TEXT
            """.trimIndent()
        )
    }
}
