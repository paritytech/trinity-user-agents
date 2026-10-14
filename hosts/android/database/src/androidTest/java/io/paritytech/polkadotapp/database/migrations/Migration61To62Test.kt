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
class Migration61To62Test {
    @get:Rule
    val helper = MigrationTestHelper(
        InstrumentationRegistry.getInstrumentation(),
        AppDatabase::class.java
    )

    @Test
    fun legacyVouchersKeepTheirLocationWithoutInventingAnInclusionTime() {
        helper.createDatabase(TEST_DB, 61).apply {
            execSQL("INSERT INTO recycler_vouchers VALUES (1, X'07', 3, 2, 32)")
            execSQL("INSERT INTO recycler_vouchers VALUES (2, X'08', 4, NULL, NULL)")
            close()
        }

        val migrated = helper.runMigrationsAndValidate(TEST_DB, 62, true)
        try {
            assertEquals(
                listOf(listOf(1L, 7L, 3L, 2L, 32L, null), listOf(2L, 8L, 4L, null, null, null)),
                migrated.vouchers()
            )
        } finally {
            migrated.close()
        }
    }

    private fun SupportSQLiteDatabase.vouchers(): List<List<Long?>> {
        val cursor = query(
            "SELECT ringVrfKeyIndex, ringVrfPublicKey, recyclerValue, locationRecyclerIndex, recyclerMembers, enteredAt " +
                "FROM recycler_vouchers ORDER BY ringVrfKeyIndex"
        )
        val rows = mutableListOf<List<Long?>>()
        while (cursor.moveToNext()) {
            rows += (0 until cursor.columnCount).map { column ->
                when {
                    cursor.isNull(column) -> null
                    column == 1 -> cursor.getBlob(column).single().toLong()
                    else -> cursor.getLong(column)
                }
            }
        }
        cursor.close()
        return rows
    }

    private companion object {
        const val TEST_DB = "voucher-migration-test"
    }
}
