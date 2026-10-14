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
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

// Legacy coinage state is dropped, not re-keyed: its keys sit on the one page every earlier install shared, so
// allocating next to them risks a collision no installation id can prevent. Anything that points at such an
// asset has to go with it.
@RunWith(AndroidJUnit4::class)
class Migration62To63Test {

    @get:Rule
    val helper = MigrationTestHelper(
        InstrumentationRegistry.getInstrumentation(),
        AppDatabase::class.java
    )

    private lateinit var db: SupportSQLiteDatabase
    private lateinit var migratedDb: SupportSQLiteDatabase

    @Before
    fun setup() {
        db = helper.createDatabase(TEST_DB, 62)
    }

    @After
    fun tearDown() {
        if (::migratedDb.isInitialized) migratedDb.close()
    }

    @Test
    fun everyLegacyCoinageRowIsDropped() {
        givenLegacyCoinageState()

        val migrated = migrate()

        COINAGE_TABLES.forEach { table -> assertEquals("$table still holds legacy rows", 0, migrated.count(table)) }
    }

    @Test
    fun onlyCoinageTransactionsLeaveTheDurableLedger() {
        givenLegacyCoinageState()
        insertDurableTx(id = 2, domainId = "other-domain")

        val migrated = migrate()

        assertEquals(1, migrated.count("durable_tx"))
        assertEquals("other-domain", migrated.string("SELECT domainId FROM durable_tx"))
    }

    @Test
    fun externalPaymentsAreDroppedWithTheVouchersTheyReference() {
        db.insert(
            "external_payments",
            ContentValues().apply {
                put("id", "payment")
                put("origin", "product")
                put("amountPlanks", "100")
                put("destination", byteArrayOf(0x0a))
                put("stage", "OFFBOARD")
                put("selectedVoucherKeys", "[2]")
                put("createdAt", 1L)
                put("updatedAt", 1L)
            }
        )

        assertEquals(0, migrate().count("external_payments"))
    }

    private fun givenLegacyCoinageState() {
        db.insert(
            "coins",
            ContentValues().apply {
                put("derivationIndex", 5)
                put("accountId", byteArrayOf(0x05))
                put("valueExponent", 3)
                put("ageValue", 9)
                put("onChain", 1)
            }
        )
        db.insert(
            "recycler_vouchers",
            ContentValues().apply {
                put("ringVrfKeyIndex", 2)
                put("ringVrfPublicKey", byteArrayOf(0x02))
                put("recyclerValue", 4)
                put("locationRecyclerIndex", 1)
                put("recyclerMembers", 12)
                put("enteredAt", 1_757_000_000_000L)
            }
        )
        insertDurableTx(id = 1, domainId = "coinage")
        db.insert(
            "coinage_entry_input",
            ContentValues().apply {
                put("entryId", 1)
                put("position", 0)
                put("assetKind", "COIN")
                put("derivationIndex", 5)
                put("onChainKey", byteArrayOf(0x05))
            }
        )
        db.insert(
            "coinage_entry_output",
            ContentValues().apply {
                put("entryId", 1)
                put("position", 0)
                put("assetKind", "COIN")
                put("derivationIndex", 6)
                put("onChainKey", byteArrayOf(0x06))
            }
        )
        db.insert(
            "coinage_handoff",
            ContentValues().apply {
                put("onChainKey", byteArrayOf(0x08))
                put("assetKind", "COIN")
                put("derivationIndex", 8)
                put("committed", 1)
            }
        )
    }

    private fun insertDurableTx(id: Long, domainId: String) {
        db.insert(
            "durable_tx",
            ContentValues().apply {
                put("id", id)
                put("domainId", domainId)
                put("txHash", "0x$id")
                put("mortalityBlocks", 64)
                put("status", "PENDING")
                put("checkpointblockNumber", 100)
                put("checkpointblockHash", "0xcheckpoint")
            }
        )
    }

    private fun SupportSQLiteDatabase.insert(table: String, values: ContentValues) {
        insert(table, SQLiteDatabase.CONFLICT_ABORT, values)
    }

    private fun SupportSQLiteDatabase.count(table: String): Int {
        val cursor = query("SELECT COUNT(*) FROM `$table`")
        cursor.moveToFirst()
        val count = cursor.getInt(0)
        cursor.close()
        return count
    }

    private fun SupportSQLiteDatabase.string(sql: String): String {
        val cursor = query(sql)
        cursor.moveToFirst()
        val value = cursor.getString(0)
        cursor.close()
        return value
    }

    private fun migrate(): SupportSQLiteDatabase {
        db.close()
        migratedDb = helper.runMigrationsAndValidate(TEST_DB, 63, true, Migration62To63())

        return migratedDb
    }

    private companion object {
        const val TEST_DB = "migration-62-to-63-test"

        val COINAGE_TABLES = listOf(
            "coins",
            "recycler_vouchers",
            "coinage_entry_input",
            "coinage_entry_output",
            "coinage_handoff",
            "coinage_installations",
        )
    }
}
