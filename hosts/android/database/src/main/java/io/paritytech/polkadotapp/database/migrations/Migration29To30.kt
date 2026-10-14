package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

class Migration29To30 : Migration(29, 30) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL(
            """
            CREATE TABLE IF NOT EXISTS product_integrations (
                productId TEXT NOT NULL,
                type TEXT NOT NULL,
                metadata TEXT,
                PRIMARY KEY(productId, type),
                FOREIGN KEY(productId) REFERENCES products(id) ON DELETE CASCADE
            )
            """.trimIndent()
        )

        // Recreate products table: remove appUrl/firstVisitedAt, add contentHash
        db.execSQL("DROP TABLE IF EXISTS products")
        db.execSQL(
            """
            CREATE TABLE IF NOT EXISTS products (
                id TEXT NOT NULL PRIMARY KEY,
                name TEXT NOT NULL,
                scriptUrl TEXT NOT NULL,
                contentHash TEXT
            )
            """.trimIndent()
        )

        // Clear legacy permission grants — incompatible with new ProductId format
        db.execSQL("DELETE FROM product_permission_grants")
    }
}
