package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

class Migration17To18 : Migration(17, 18) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL(
            """
            ALTER TABLE chains ADD COLUMN genesisHash TEXT NOT NULL DEFAULT ''
            """.trimIndent()
        )

        db.execSQL(
            """
            UPDATE chains SET genesisHash = id
            """.trimIndent()
        )
    }
}
