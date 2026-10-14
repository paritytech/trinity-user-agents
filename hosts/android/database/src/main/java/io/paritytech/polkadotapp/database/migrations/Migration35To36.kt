package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

class Migration35To36 : Migration(35, 36) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE file_uploads ADD COLUMN nodeUrl TEXT NOT NULL DEFAULT ''")
        db.execSQL("ALTER TABLE file_downloads ADD COLUMN nodeUrl TEXT NOT NULL DEFAULT ''")

        // Drop sourceType column from chain_external_apis. SQLite doesn't support DROP COLUMN
        // directly on older versions so rebuild the table.
        db.execSQL(
            """
            CREATE TABLE chain_external_apis_new (
                chainId TEXT NOT NULL,
                apiType TEXT NOT NULL,
                parameters TEXT,
                url TEXT NOT NULL,
                PRIMARY KEY(chainId, url, apiType),
                FOREIGN KEY(chainId) REFERENCES chains(id) ON UPDATE NO ACTION ON DELETE CASCADE
            )
            """.trimIndent()
        )
        db.execSQL(
            """
            INSERT INTO chain_external_apis_new (chainId, apiType, parameters, url)
            SELECT chainId, apiType, parameters, url FROM chain_external_apis
            """.trimIndent()
        )
        db.execSQL("DROP TABLE chain_external_apis")
        db.execSQL("ALTER TABLE chain_external_apis_new RENAME TO chain_external_apis")
        db.execSQL("CREATE INDEX IF NOT EXISTS index_chain_external_apis_chainId ON chain_external_apis(chainId)")
    }
}
