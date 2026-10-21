package io.paritytech.polkadotapp.database.migrations

import androidx.room.testing.MigrationTestHelper
import androidx.sqlite.db.SupportSQLiteDatabase
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.paritytech.polkadotapp.database.AppDatabase
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class Migration71To72Test {
    @get:Rule
    val helper = MigrationTestHelper(
        InstrumentationRegistry.getInstrumentation(),
        AppDatabase::class.java
    )

    // A message stored before Chat Extension v2 has no participant. A room starts with no declarations,
    // so it keeps the host defaults until its product declares something.
    @Test
    fun messagesKeepTheirRowsAndRoomsStartWithoutDeclarations() {
        helper.createDatabase(TEST_DB, 71).apply {
            execSQL(
                "INSERT INTO chat_messages (id, chatId, timestamp, updatedAt, sortOrder, status, type, searchableContent, " +
                    "content, replyToMessageId, isInternal, origintype, originkey) " +
                    "VALUES ('m1', X'01', 1, 1, 1, 'NEW', 'TEXT', 'hi', X'00', NULL, 0, 'MIDDLEWARE', X'02')"
            )
            close()
        }

        val migrated = helper.runMigrationsAndValidate(TEST_DB, 72, true)
        try {
            assertEquals(listOf(listOf("m1", "hi", null)), migrated.rows("SELECT id, searchableContent, originParticipantId FROM chat_messages"))
            assertEquals(emptyList<List<String?>>(), migrated.rows("SELECT chatId FROM chat_room_declarations"))

            migrated.execSQL("INSERT INTO chat_room_declarations (chatId, description) VALUES (X'01', 'A room')")
            assertEquals(listOf(listOf("0", "A room")), migrated.rows("SELECT configGeneration, description FROM chat_room_declarations"))
        } finally {
            migrated.close()
        }
    }

    private fun SupportSQLiteDatabase.rows(sql: String): List<List<String?>> = query(sql).use { cursor ->
        buildList {
            while (cursor.moveToNext()) add((0 until cursor.columnCount).map(cursor::getString))
        }
    }

    private companion object {
        const val TEST_DB = "chat-room-declarations-migration-test"
    }
}
