package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

/**
 * The coinage ledger becomes the durability engine's ledger, shared by every domain.
 *
 * Ids are carried over unchanged: `coinage_entry_input`, `coinage_entry_output` and the asset-state query
 * all reference `entryId`, and those rows stay exactly where they are. Only the row describing the
 * transaction itself moves, gaining the domain that owns it.
 */
class Migration60To61 : Migration(60, 61) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL(CREATE_DURABLE_TX)
        db.execSQL(INDEX_DOMAIN_GROUP)
        db.execSQL(INDEX_DOMAIN_STATUS)

        // Explicit ids, which is also what carries `sqlite_sequence` forward for the AUTOINCREMENT column:
        // SQLite advances it whenever a row lands above the current maximum, so a later insert cannot reuse
        // one — and a domain's asset rows are keyed on exactly these. `Migration60To61Test` pins that.
        db.execSQL(
            """
            INSERT INTO `durable_tx` (
                `id`, `domainId`, `operationGroupId`, `txHash`, `mortalityBlocks`, `status`,
                `checkpointblockNumber`, `checkpointblockHash`,
                `successDetectedblockNumber`, `successDetectedblockHash`
            )
            SELECT
                `id`, '$COINAGE_DOMAIN_ID', `operationGroupId`, `txHash`, `mortalityBlocks`, `status`,
                `checkpointblockNumber`, `checkpointblockHash`,
                `successDetectedblockNumber`, `successDetectedblockHash`
            FROM `coinage_entry`
            """.trimIndent()
        )

        db.execSQL("DROP TABLE `coinage_entry`")
    }

    private companion object {
        /**
         * Structurally what Room generates for `DurableTxLocal`; see `schemas/61.json`.
         *
         * Only the shape has to match: Room validates the table it finds after the migration, not the
         * statement that produced it, so the layout here is free to be readable.
         */
        val CREATE_DURABLE_TX = """
            CREATE TABLE IF NOT EXISTS `durable_tx` (
                `id` INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                `domainId` TEXT NOT NULL,
                `operationGroupId` TEXT,
                `txHash` TEXT NOT NULL,
                `mortalityBlocks` INTEGER NOT NULL,
                `status` TEXT NOT NULL,
                `checkpointblockNumber` INTEGER NOT NULL,
                `checkpointblockHash` TEXT NOT NULL,
                `successDetectedblockNumber` INTEGER,
                `successDetectedblockHash` TEXT
            )
        """.trimIndent()

        val INDEX_DOMAIN_GROUP = """
            CREATE INDEX IF NOT EXISTS `index_durable_tx_domainId_operationGroupId`
            ON `durable_tx` (`domainId`, `operationGroupId`)
        """.trimIndent()

        val INDEX_DOMAIN_STATUS = """
            CREATE INDEX IF NOT EXISTS `index_durable_tx_domainId_status`
            ON `durable_tx` (`domainId`, `status`)
        """.trimIndent()

        const val COINAGE_DOMAIN_ID = "coinage"
    }
}
