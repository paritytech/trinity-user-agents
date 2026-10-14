package io.paritytech.polkadotapp.database.migrations

import android.content.ContentValues
import android.database.sqlite.SQLiteDatabase
import androidx.room.testing.MigrationTestHelper
import androidx.sqlite.db.SupportSQLiteDatabase
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.paritytech.polkadotapp.database.AppDatabase
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

// The ledger table is copied to relax its attempt columns. Domain rows reference transaction ids, so every row
// must survive with its id, and ids handed out afterwards must never reuse one.
@RunWith(AndroidJUnit4::class)
class Migration65To66Test {

    @get:Rule
    val helper = MigrationTestHelper(
        InstrumentationRegistry.getInstrumentation(),
        AppDatabase::class.java
    )

    private lateinit var db: SupportSQLiteDatabase
    private lateinit var migratedDb: SupportSQLiteDatabase

    @Before
    fun setup() {
        db = helper.createDatabase(TEST_DB, 65)
    }

    @After
    fun tearDown() {
        if (::migratedDb.isInitialized) migratedDb.close()
    }

    @Test
    fun everyTransactionKeepsItsIdAndAttemptAndGetsNoPolicy() {
        insertDurableTx(id = 3, status = "FINALIZED_SUCCESS")
        insertDurableTx(id = 7, status = "PENDING")

        val migrated = migrate()

        assertEquals(2, migrated.count("durable_tx"))
        assertEquals("0xhash7", migrated.string("SELECT txHash FROM durable_tx WHERE id = 7"))
        assertEquals("PENDING", migrated.string("SELECT status FROM durable_tx WHERE id = 7"))
        assertEquals(0, migrated.count("durable_tx", "submissionPolicyId IS NOT NULL OR submissionPolicyParams IS NOT NULL"))
    }

    @Test
    fun idsKeepGrowingPastTheCopiedRows() {
        insertDurableTx(id = 7, status = "FAILURE")

        val migrated = migrate()
        migrated.insert(
            "durable_tx",
            SQLiteDatabase.CONFLICT_ABORT,
            ContentValues().apply {
                put("domainId", "coinage")
                put("status", "PENDING_SUBMISSION")
                put("submissionPolicyId", "coinage-split")
                put("submissionPolicyParams", byteArrayOf(0))
            }
        )

        assertTrue(migrated.long("SELECT MAX(id) FROM durable_tx") > 7)
    }

    private fun insertDurableTx(id: Long, status: String) {
        db.insert(
            "durable_tx",
            SQLiteDatabase.CONFLICT_ABORT,
            ContentValues().apply {
                put("id", id)
                put("domainId", "coinage")
                put("operationGroupId", "group")
                put("txHash", "0xhash$id")
                put("mortalityBlocks", 64L)
                put("status", status)
                put("checkpointblockNumber", 10L)
                put("checkpointblockHash", "0x10")
            }
        )
    }

    private fun SupportSQLiteDatabase.count(table: String, where: String = "1"): Int {
        query("SELECT COUNT(*) FROM $table WHERE $where").use { cursor ->
            cursor.moveToFirst()
            return cursor.getInt(0)
        }
    }

    private fun SupportSQLiteDatabase.string(sql: String): String {
        query(sql).use { cursor ->
            cursor.moveToFirst()
            return cursor.getString(0)
        }
    }

    private fun SupportSQLiteDatabase.long(sql: String): Long {
        query(sql).use { cursor ->
            cursor.moveToFirst()
            return cursor.getLong(0)
        }
    }

    private fun migrate(): SupportSQLiteDatabase {
        db.close()
        migratedDb = helper.runMigrationsAndValidate(TEST_DB, 66, true, Migration65To66())
        return migratedDb
    }

    private companion object {
        const val TEST_DB = "migration-65-to-66-test"
    }
}
