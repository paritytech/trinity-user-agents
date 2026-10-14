package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

class Migration38To39 : Migration(38, 39) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL(
            """
            CREATE TABLE IF NOT EXISTS statement_store_slot_allocations (
                id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
                chainId TEXT NOT NULL,
                collection TEXT NOT NULL,
                accountId BLOB NOT NULL,
                seq INTEGER NOT NULL,
                latestRenewedPeriod INTEGER NOT NULL,
                sinceMillis INTEGER NOT NULL,
                priorityLevel INTEGER NOT NULL
            )
            """.trimIndent()
        )
        db.execSQL(
            "CREATE INDEX IF NOT EXISTS index_statement_store_slot_allocations_chainId_collection " +
                "ON statement_store_slot_allocations(chainId, collection)"
        )
        db.execSQL(
            "CREATE INDEX IF NOT EXISTS index_statement_store_slot_allocations_chainId_collection_accountId " +
                "ON statement_store_slot_allocations(chainId, collection, accountId)"
        )
    }
}
