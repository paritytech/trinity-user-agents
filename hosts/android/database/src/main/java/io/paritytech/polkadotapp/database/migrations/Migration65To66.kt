package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

/**
 * Durable transactions can be scheduled before they are built, and carry the policy that builds them again.
 *
 * SQLite cannot relax a NOT NULL constraint in place, so the table is copied. Every existing row keeps its id
 * — domain rows reference it — and gets no policy, which is exactly what it was registered with.
 */
class Migration65To66 : Migration(65, 66) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL(
            """
            CREATE TABLE IF NOT EXISTS `durable_tx_new` (
                `id` INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                `domainId` TEXT NOT NULL,
                `operationGroupId` TEXT,
                `txHash` TEXT,
                `mortalityBlocks` INTEGER,
                `status` TEXT NOT NULL,
                `submissionPolicyId` TEXT,
                `submissionPolicyParams` BLOB,
                `checkpointblockNumber` INTEGER,
                `checkpointblockHash` TEXT,
                `successDetectedblockNumber` INTEGER,
                `successDetectedblockHash` TEXT
            )
            """.trimIndent()
        )

        db.execSQL(
            """
            INSERT INTO `durable_tx_new` (
                `id`, `domainId`, `operationGroupId`, `txHash`, `mortalityBlocks`, `status`,
                `checkpointblockNumber`, `checkpointblockHash`, `successDetectedblockNumber`, `successDetectedblockHash`
            )
            SELECT
                `id`, `domainId`, `operationGroupId`, `txHash`, `mortalityBlocks`, `status`,
                `checkpointblockNumber`, `checkpointblockHash`, `successDetectedblockNumber`, `successDetectedblockHash`
            FROM `durable_tx`
            """.trimIndent()
        )

        db.execSQL("DROP TABLE `durable_tx`")
        db.execSQL("ALTER TABLE `durable_tx_new` RENAME TO `durable_tx`")

        db.execSQL(
            "CREATE INDEX IF NOT EXISTS `index_durable_tx_domainId_operationGroupId` " +
                "ON `durable_tx` (`domainId`, `operationGroupId`)"
        )
        db.execSQL("CREATE INDEX IF NOT EXISTS `index_durable_tx_domainId_status` ON `durable_tx` (`domainId`, `status`)")
    }
}
