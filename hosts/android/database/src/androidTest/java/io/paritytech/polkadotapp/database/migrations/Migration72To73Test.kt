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
class Migration72To73Test {
    @get:Rule
    val helper = MigrationTestHelper(
        InstrumentationRegistry.getInstrumentation(),
        AppDatabase::class.java
    )

    // The migration only adds the declarations table, so the messages of a room keep their rows. A room starts with no
    // declarations, and a new row starts at configuration generation 0.
    @Test
    fun keepsTheMessagesAndStartsWithoutDeclarations() {
        helper.createDatabase(TEST_DB, 72).apply {
            execSQL(
                "INSERT INTO chat_messages (id, chatId, timestamp, updatedAt, sortOrder, status, type, searchableContent, " +
                    "content, replyToMessageId, isInternal, origintype, originkey) " +
                    "VALUES ('m1', X'01', 1, 1, 1, 'NEW', 'TEXT', 'hi', X'00', NULL, 0, 'MIDDLEWARE', X'02')"
            )
            close()
        }

        val migrated = helper.runMigrationsAndValidate(TEST_DB, 73, true)
        try {
            assertEquals(listOf(listOf("m1", "hi")), migrated.rows("SELECT id, searchableContent FROM chat_messages"))
            assertEquals(emptyList<List<String?>>(), migrated.rows("SELECT chatId FROM chat_room_declarations"))

            migrated.execSQL("INSERT INTO chat_room_declarations (chatId, description, placeholder) VALUES (X'01', 'A room', 'No games yet')")
            assertEquals(
                listOf(listOf("0", "A room", "No games yet")),
                migrated.rows("SELECT configGeneration, description, placeholder FROM chat_room_declarations"),
            )
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
