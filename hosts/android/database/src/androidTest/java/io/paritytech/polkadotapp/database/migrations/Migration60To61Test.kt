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

/**
 * The coinage ledger becoming the engine's ledger, with rows in it.
 *
 * `MigrationTest.migrateAll` runs this migration over an empty database, which proves the statements parse
 * and that Room accepts the resulting schema. It cannot see the part that matters: the rows have to arrive
 * under their original ids, because `coinage_entry_input` and `coinage_entry_output` reference them and
 * nothing rewrites those references.
 */
@RunWith(AndroidJUnit4::class)
class Migration60To61Test {

    @get:Rule
    val helper = MigrationTestHelper(
        InstrumentationRegistry.getInstrumentation(),
        AppDatabase::class.java
    )

    private lateinit var db: SupportSQLiteDatabase
    private lateinit var migratedDb: SupportSQLiteDatabase

    @Before
    fun setup() {
        db = helper.createDatabase(TEST_DB, 60)
    }

    @After
    fun tearDown() {
        if (::migratedDb.isInitialized) migratedDb.close()
    }

    @Test
    fun everyEntryArrivesUnderTheCoinageDomain() {
        db.insertEntry(id = 1, status = "PENDING")
        db.insertEntry(id = 2, status = "FINALIZED_SUCCESS")

        val migrated = migrate()

        assertEquals(2, migrated.count("SELECT COUNT(*) FROM durable_tx WHERE domainId = 'coinage'"))
    }

    /** The reference that is never rewritten, so it has to still resolve after the move. */
    @Test
    fun idsAreCarriedOverSoAssetRowsStillResolve() {
        db.insertEntry(id = 7, status = "PENDING")
        db.insertOutput(entryId = 7, derivationIndex = 3)

        val migrated = migrate()

        val joined = migrated.query(
            """
            SELECT o.derivationIndex FROM coinage_entry_output o
            JOIN durable_tx e ON e.id = o.entryId
            WHERE e.id = 7
            """.trimIndent()
        )
        assertTrue("the output no longer joins to its transaction", joined.moveToFirst())
        assertEquals(3, joined.getInt(0))
        joined.close()
    }

    @Test
    fun everyColumnSurvivesTheMove() {
        db.insertEntry(
            id = 4,
            status = "PENDING_SUCCESS",
            groupId = "group-a",
            txHash = "0xabc",
            successDetectedNumber = 140,
            successDetectedHash = "0xdetected",
        )

        val migrated = migrate()

        val row = migrated.query(
            """
            SELECT operationGroupId, txHash, mortalityBlocks, status,
                   checkpointblockNumber, checkpointblockHash,
                   successDetectedblockNumber, successDetectedblockHash
            FROM durable_tx WHERE id = 4
            """.trimIndent()
        )
        assertTrue(row.moveToFirst())
        assertEquals("group-a", row.getString(0))
        assertEquals("0xabc", row.getString(1))
        assertEquals(64, row.getLong(2))
        assertEquals("PENDING_SUCCESS", row.getString(3))
        assertEquals(100, row.getLong(4))
        assertEquals("0xcheckpoint", row.getString(5))
        assertEquals(140, row.getLong(6))
        assertEquals("0xdetected", row.getString(7))
        row.close()
    }

    /**
     * The property that matters, whatever guarantees it.
     *
     * A reused id hands one transaction another's asset rows, silently. Nothing in the migration arranges
     * this: SQLite advances `sqlite_sequence` itself when the copy inserts explicit rowids above the
     * current maximum. That is exactly why it is worth a test — the guarantee lives outside the code, so a
     * rewritten copy could lose it without a line of the migration looking wrong.
     */
    @Test
    fun theNextInsertCannotReuseACarriedOverId() {
        db.insertEntry(id = 1, status = "PENDING")
        db.insertEntry(id = 42, status = "PENDING")

        val migrated = migrate()

        migrated.insert(
            "durable_tx",
            SQLiteDatabase.CONFLICT_NONE,
            ContentValues().apply {
                put("domainId", "other-domain")
                put("txHash", "0xnew")
                put("mortalityBlocks", 64L)
                put("status", "PENDING")
                put("checkpointblockNumber", 200L)
                put("checkpointblockHash", "0xcheckpoint")
            }
        )

        val assigned = migrated.count("SELECT id FROM durable_tx WHERE txHash = '0xnew'")
        assertTrue("a fresh insert reused id $assigned, which an asset row already points at", assigned > 42)
    }

    @Test
    fun theOldTableIsGone() {
        db.insertEntry(id = 1, status = "PENDING")

        val migrated = migrate()

        assertEquals(
            0,
            migrated.count("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'coinage_entry'")
        )
    }

    // ---- fixtures ----

    private fun SupportSQLiteDatabase.insertEntry(
        id: Long,
        status: String,
        groupId: String? = null,
        txHash: String = "0xtx$id",
        successDetectedNumber: Long? = null,
        successDetectedHash: String? = null,
    ) {
        val values = ContentValues().apply {
            put("id", id)
            put("operationGroupId", groupId)
            put("txHash", txHash)
            put("mortalityBlocks", 64L)
            put("status", status)
            put("checkpointblockNumber", 100L)
            put("checkpointblockHash", "0xcheckpoint")
            put("successDetectedblockNumber", successDetectedNumber)
            put("successDetectedblockHash", successDetectedHash)
        }
        insert("coinage_entry", SQLiteDatabase.CONFLICT_NONE, values)
    }

    private fun SupportSQLiteDatabase.insertOutput(entryId: Long, derivationIndex: Int) {
        val values = ContentValues().apply {
            put("entryId", entryId)
            put("position", 0)
            put("assetKind", "COIN")
            put("derivationIndex", derivationIndex)
            put("onChainKey", byteArrayOf(derivationIndex.toByte()))
        }
        insert("coinage_entry_output", SQLiteDatabase.CONFLICT_NONE, values)
    }

    private fun SupportSQLiteDatabase.count(sql: String): Long {
        val cursor = query(sql)
        cursor.moveToFirst()
        val value = cursor.getLong(0)
        cursor.close()

        return value
    }

    private fun migrate(): SupportSQLiteDatabase {
        db.close()
        migratedDb = helper.runMigrationsAndValidate(TEST_DB, 61, true, Migration60To61())

        return migratedDb
    }

    private companion object {
        const val TEST_DB = "migration-60-to-61-test"
    }
}
