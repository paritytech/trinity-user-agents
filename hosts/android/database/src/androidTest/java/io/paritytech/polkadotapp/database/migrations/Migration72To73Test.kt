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

    // A room declared before the additions has no features and no user state, so it keeps the host defaults.
    @Test
    fun declarationsKeepTheirValuesAndStartWithoutFeaturesOrUserState() {
        helper.createDatabase(TEST_DB, 72).apply {
            execSQL(
                "INSERT INTO chat_room_declarations (chatId, config, configGeneration, description) " +
                    "VALUES (X'01', X'0000000000000000', 3, 'A room')"
            )
            close()
        }

        val migrated = helper.runMigrationsAndValidate(TEST_DB, 73, true)
        try {
            assertEquals(
                listOf(listOf("3", "A room", null, null, "0")),
                migrated.rows("SELECT configGeneration, description, features, mutedUntil, archived FROM chat_room_declarations"),
            )
            assertEquals(emptyList<List<String?>>(), migrated.rows("SELECT chatId FROM chat_message_meta"))
            assertEquals(emptyList<List<String?>>(), migrated.rows("SELECT botId FROM chat_bots"))

            migrated.execSQL("INSERT INTO chat_room_declarations (chatId, configGeneration, mutedUntil) VALUES (X'02', 0, 9)")
            migrated.execSQL("INSERT INTO chat_message_meta (chatId, messageId, timestamp, meta) VALUES (X'02', 'm1', 5, X'0100')")
            migrated.execSQL("INSERT INTO chat_bots (extensionId, botId, name, icon) VALUES ('ext', 'helper', 'Helper', 'data:')")
            assertEquals(listOf(listOf("9", "0")), migrated.rows("SELECT mutedUntil, archived FROM chat_room_declarations WHERE chatId = X'02'"))
            assertEquals(listOf(listOf("m1", "5")), migrated.rows("SELECT messageId, timestamp FROM chat_message_meta"))
            assertEquals(listOf(listOf("helper", "Helper")), migrated.rows("SELECT botId, name FROM chat_bots"))
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
        const val TEST_DB = "chat-extension-additions-migration-test"
    }
}
