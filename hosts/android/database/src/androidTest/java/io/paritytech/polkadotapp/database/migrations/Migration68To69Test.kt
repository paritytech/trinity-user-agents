package io.paritytech.polkadotapp.database.migrations

import androidx.room.testing.MigrationTestHelper
import androidx.sqlite.db.SupportSQLiteDatabase
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.paritytech.polkadotapp.database.AppDatabase
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class Migration68To69Test {
    @get:Rule
    val helper = MigrationTestHelper(
        InstrumentationRegistry.getInstrumentation(),
        AppDatabase::class.java
    )

    // A face kept as JSON of the old widget model cannot be read as the core's tree, so it goes. The
    // card the user added must not go with it: losing a picture until the product draws again is the
    // cost of the move, losing the card is not.
    @Test
    fun cardsKeepTheirPlaceWhileTheirOldFacesAreDropped() {
        helper.createDatabase(TEST_DB, 68).apply {
            execSQL("INSERT INTO pocket_cards VALUES ('game.dot', 'loyalty', 'Loyalty')")
            execSQL("INSERT INTO pocket_card_faces VALUES ('game.dot', 'loyalty', '{\"type\":\"text\"}')")
            close()
        }

        val migrated = helper.runMigrationsAndValidate(TEST_DB, 69, true, Migration68To69())
        try {
            assertEquals(listOf(listOf("game.dot", "loyalty", "Loyalty")), migrated.rows("SELECT * FROM pocket_cards"))
            assertEquals(emptyList<List<String>>(), migrated.rows("SELECT productId, cardId FROM pocket_card_faces"))

            migrated.execSQL("INSERT INTO pocket_card_faces VALUES ('game.dot', 'loyalty', X'0A0B')")
            migrated.query("SELECT face FROM pocket_card_faces").use { faces ->
                faces.moveToFirst()
                assertArrayEquals(byteArrayOf(0x0A, 0x0B), faces.getBlob(0))
            }
        } finally {
            migrated.close()
        }
    }

    private fun SupportSQLiteDatabase.rows(sql: String): List<List<String>> = query(sql).use { cursor ->
        buildList {
            while (cursor.moveToNext()) add((0 until cursor.columnCount).map(cursor::getString))
        }
    }

    private companion object {
        const val TEST_DB = "pocket-face-migration-test"
    }
}
