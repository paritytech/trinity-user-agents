package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

// Coinage keys become scoped to the installation that allocated them. Legacy state is dropped rather than re-keyed:
// its keys sit on the single `//0` page every earlier install of the seed allocated in, so any key next to them
// could collide with one another install already handed off. Everything that points at such an asset goes with it —
// the ledger rows, coinage's durable transactions and external payments holding voucher keys.
class Migration62To63 : Migration(62, 63) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("DELETE FROM `durable_tx` WHERE `domainId` = 'coinage'")
        db.execSQL("DELETE FROM `external_payments`")

        recreateEmpty(
            db = db,
            table = "coins",
            createSql = """
                CREATE TABLE `coins` (
                    `installationId` BLOB NOT NULL,
                    `derivationIndex` INTEGER NOT NULL,
                    `accountId` BLOB NOT NULL,
                    `valueExponent` INTEGER NOT NULL,
                    `ageValue` INTEGER,
                    `onChain` INTEGER NOT NULL,
                    PRIMARY KEY(`installationId`, `derivationIndex`)
                )
            """,
        )

        recreateEmpty(
            db = db,
            table = "recycler_vouchers",
            createSql = """
                CREATE TABLE `recycler_vouchers` (
                    `installationId` BLOB NOT NULL,
                    `ringVrfKeyIndex` INTEGER NOT NULL,
                    `ringVrfPublicKey` BLOB NOT NULL,
                    `recyclerValue` INTEGER NOT NULL,
                    `locationRecyclerIndex` INTEGER,
                    `recyclerMembers` INTEGER,
                    `enteredAt` INTEGER,
                    PRIMARY KEY(`installationId`, `ringVrfKeyIndex`)
                )
            """,
        )

        recreateEmpty(
            db = db,
            table = "coinage_entry_input",
            createSql = """
                CREATE TABLE `coinage_entry_input` (
                    `entryId` INTEGER NOT NULL,
                    `position` INTEGER NOT NULL,
                    `assetKind` TEXT NOT NULL,
                    `installationId` BLOB,
                    `derivationIndex` INTEGER,
                    `onChainKey` BLOB NOT NULL,
                    PRIMARY KEY(`entryId`, `position`)
                )
            """,
        )
        db.execSQL("CREATE INDEX IF NOT EXISTS `index_coinage_entry_input_onChainKey` ON `coinage_entry_input` (`onChainKey`)")
        db.execSQL("CREATE INDEX IF NOT EXISTS `index_coinage_entry_input_entryId` ON `coinage_entry_input` (`entryId`)")

        recreateEmpty(
            db = db,
            table = "coinage_entry_output",
            createSql = """
                CREATE TABLE `coinage_entry_output` (
                    `entryId` INTEGER NOT NULL,
                    `position` INTEGER NOT NULL,
                    `assetKind` TEXT NOT NULL,
                    `installationId` BLOB NOT NULL,
                    `derivationIndex` INTEGER NOT NULL,
                    `onChainKey` BLOB NOT NULL,
                    PRIMARY KEY(`entryId`, `position`)
                )
            """,
        )
        db.execSQL("CREATE UNIQUE INDEX IF NOT EXISTS `index_coinage_entry_output_onChainKey` ON `coinage_entry_output` (`onChainKey`)")
        db.execSQL("CREATE INDEX IF NOT EXISTS `index_coinage_entry_output_entryId` ON `coinage_entry_output` (`entryId`)")
        db.execSQL(
            "CREATE INDEX IF NOT EXISTS `index_coinage_entry_output_assetKind_installationId_derivationIndex` " +
                "ON `coinage_entry_output` (`assetKind`, `installationId`, `derivationIndex`)"
        )

        recreateEmpty(
            db = db,
            table = "coinage_handoff",
            createSql = """
                CREATE TABLE `coinage_handoff` (
                    `onChainKey` BLOB NOT NULL,
                    `assetKind` TEXT NOT NULL,
                    `installationId` BLOB NOT NULL,
                    `derivationIndex` INTEGER NOT NULL,
                    `committed` INTEGER NOT NULL,
                    PRIMARY KEY(`onChainKey`)
                )
            """,
        )
        db.execSQL(
            "CREATE INDEX IF NOT EXISTS `index_coinage_handoff_assetKind_installationId_derivationIndex` " +
                "ON `coinage_handoff` (`assetKind`, `installationId`, `derivationIndex`)"
        )

        db.execSQL(
            """
            CREATE TABLE IF NOT EXISTS `coinage_installations` (
                `installationId` BLOB NOT NULL,
                `isCurrent` INTEGER NOT NULL,
                `coinScanNextIndex` INTEGER NOT NULL,
                `voucherScanNextIndex` INTEGER NOT NULL,
                `initialScanCompleted` INTEGER NOT NULL,
                PRIMARY KEY(`installationId`)
            )
            """.trimIndent()
        )
    }

    private fun recreateEmpty(db: SupportSQLiteDatabase, table: String, createSql: String) {
        db.execSQL("DROP TABLE `$table`")
        db.execSQL(createSql.trimIndent())
    }
}
