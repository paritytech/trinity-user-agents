package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

class Migration24To25 : Migration(24, 25) {
    override fun migrate(db: SupportSQLiteDatabase) {
        createChatRoomsTable(db)
        rewriteChatIdPrefixes(db)
        populateRoomsFromExistingBots(db)
        populateRoomsFromExistingContacts(db)
    }

    private fun createChatRoomsTable(db: SupportSQLiteDatabase) {
        db.execSQL(
            """
            CREATE TABLE IF NOT EXISTS chat_rooms (
                id BLOB NOT NULL PRIMARY KEY,
                createdAt INTEGER NOT NULL,
                name TEXT,
                icon TEXT
            )
            """.trimIndent()
        )
    }

    private fun rewriteChatIdPrefixes(db: SupportSQLiteDatabase) {
        val tables = listOf("chat_messages", "chat_message_processing", "chat_message_reactions", "message_revisions")

        for (table in tables) {
            db.execSQL(
                """
                UPDATE $table
                SET chatId = CAST(REPLACE(CAST(chatId AS TEXT), 'ChatBot:', 'ChatExtension:') AS BLOB)
                WHERE CAST(chatId AS TEXT) LIKE 'ChatBot:%'
                """.trimIndent()
            )
        }
    }

    private fun populateRoomsFromExistingBots(db: SupportSQLiteDatabase) {
        // Insert rooms for active bots using the new ChatExtension prefix
        // createdAt is derived from earliest message timestamp, or current time if no messages
        db.execSQL(
            """
            INSERT OR IGNORE INTO chat_rooms (id, createdAt, name, icon)
            SELECT
                CAST('ChatExtension:' || bs.middlewareId AS BLOB),
                COALESCE(
                    (SELECT MIN(timestamp) FROM chat_messages WHERE chatId = CAST('ChatExtension:' || bs.middlewareId AS BLOB)),
                    ${System.currentTimeMillis()}
                ),
                NULL,
                NULL
            FROM chat_bot_state bs
            WHERE bs.isActive = 1
            """.trimIndent()
        )
    }

    private fun populateRoomsFromExistingContacts(db: SupportSQLiteDatabase) {
        // Insert rooms for existing contacts using their accountId as the room id
        db.execSQL(
            """
            INSERT OR IGNORE INTO chat_rooms (id, createdAt, name, icon)
            SELECT
                c.accountId,
                COALESCE(
                    (SELECT MIN(timestamp) FROM chat_messages WHERE chatId = c.accountId),
                    ${System.currentTimeMillis()}
                ),
                NULL,
                NULL
            FROM contacts c
            """.trimIndent()
        )
    }
}
