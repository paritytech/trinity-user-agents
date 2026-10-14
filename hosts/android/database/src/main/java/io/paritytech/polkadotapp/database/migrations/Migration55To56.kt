package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

/**
 * Replaces the coinage transfer WAL and the chat-side detection table with the entry ledger.
 *
 * Nothing is carried over: neither stored the signed bytes, so an in-flight row could not be resumed even in
 * principle, and what they tracked is now derived — spent/usage state from the ledger plus on-chain presence,
 * a chat payment's progress from the group its claims were registered under. Coins and vouchers keep their
 * rows; only the overloaded state columns go.
 */
class Migration55To56 : Migration(55, 56) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.createEntryTable()
        db.createInputsTable()
        db.createOutputsTable()
        db.createHandoffTable()
        db.dropTransferWal()
        db.dropTransferDetection()
        db.dropCoinSpentState()
        db.dropVoucherUsageState()
        db.splitCoinPresenceFromAge()
    }

    /**
     * A coin's on-chain presence stops being its age.
     *
     * The two were one column: an age was known exactly while the coin was there, and cleared when it left.
     * That cannot tell a coin the chain no longer holds from one nothing has looked at yet, and the two call
     * for opposite conclusions — the first says a peer took the coin, the second that it is too early to say.
     *
     * Presence is seeded from the old meaning, which was exact for the coins that were on chain.
     */
    private fun SupportSQLiteDatabase.splitCoinPresenceFromAge() {
        execSQL("ALTER TABLE coins ADD COLUMN onChain INTEGER NOT NULL DEFAULT 0")
        execSQL("UPDATE coins SET onChain = 1 WHERE ageValue IS NOT NULL")
    }

    private fun SupportSQLiteDatabase.dropTransferWal() {
        execSQL("DROP TABLE IF EXISTS coinage_transfer_wal")
    }

    private fun SupportSQLiteDatabase.dropTransferDetection() {
        execSQL("DROP TABLE IF EXISTS chat_coinage_transfer_detection")
    }

    private fun SupportSQLiteDatabase.dropCoinSpentState() {
        execSQL("ALTER TABLE coins DROP COLUMN spentState")
    }

    private fun SupportSQLiteDatabase.dropVoucherUsageState() {
        execSQL("ALTER TABLE recycler_vouchers DROP COLUMN usageState")
    }

    private fun SupportSQLiteDatabase.createEntryTable() {
        execSQL(
            """
            CREATE TABLE IF NOT EXISTS coinage_entry (
                id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                operationGroupId TEXT,
                txHash TEXT NOT NULL,
                checkpointblockNumber INTEGER NOT NULL,
                checkpointblockHash TEXT NOT NULL,
                mortalityBlocks INTEGER NOT NULL,
                successDetectedblockNumber INTEGER,
                successDetectedblockHash TEXT,
                status TEXT NOT NULL
            )
            """.trimIndent()
        )
        execSQL("CREATE INDEX IF NOT EXISTS index_coinage_entry_operationGroupId ON coinage_entry(operationGroupId)")
        execSQL("CREATE INDEX IF NOT EXISTS index_coinage_entry_status ON coinage_entry(status)")
    }

    private fun SupportSQLiteDatabase.createInputsTable() {
        execSQL(
            """
            CREATE TABLE IF NOT EXISTS coinage_entry_input (
                entryId INTEGER NOT NULL,
                position INTEGER NOT NULL,
                assetKind TEXT NOT NULL,
                derivationIndex INTEGER,
                onChainKey BLOB NOT NULL,
                PRIMARY KEY(entryId, position)
            )
            """.trimIndent()
        )
        execSQL("CREATE INDEX IF NOT EXISTS index_coinage_entry_input_onChainKey ON coinage_entry_input(onChainKey)")
        execSQL("CREATE INDEX IF NOT EXISTS index_coinage_entry_input_entryId ON coinage_entry_input(entryId)")
    }

    private fun SupportSQLiteDatabase.createOutputsTable() {
        execSQL(
            """
            CREATE TABLE IF NOT EXISTS coinage_entry_output (
                entryId INTEGER NOT NULL,
                position INTEGER NOT NULL,
                assetKind TEXT NOT NULL,
                derivationIndex INTEGER NOT NULL,
                onChainKey BLOB NOT NULL,
                PRIMARY KEY(entryId, position)
            )
            """.trimIndent()
        )
        // Unique: the Fresh-outputs invariant, made structural.
        execSQL(
            "CREATE UNIQUE INDEX IF NOT EXISTS index_coinage_entry_output_onChainKey ON coinage_entry_output(onChainKey)"
        )
        execSQL("CREATE INDEX IF NOT EXISTS index_coinage_entry_output_entryId ON coinage_entry_output(entryId)")
        execSQL(
            """
            CREATE INDEX IF NOT EXISTS index_coinage_entry_output_assetKind_derivationIndex
            ON coinage_entry_output(assetKind, derivationIndex)
            """.trimIndent()
        )
    }

    private fun SupportSQLiteDatabase.createHandoffTable() {
        execSQL(
            """
            CREATE TABLE IF NOT EXISTS coinage_handoff (
                onChainKey BLOB NOT NULL,
                assetKind TEXT NOT NULL,
                derivationIndex INTEGER NOT NULL,
                committed INTEGER NOT NULL,
                PRIMARY KEY(onChainKey)
            )
            """.trimIndent()
        )
        execSQL(
            """
            CREATE INDEX IF NOT EXISTS index_coinage_handoff_assetKind_derivationIndex
            ON coinage_handoff(assetKind, derivationIndex)
            """.trimIndent()
        )
    }
}
