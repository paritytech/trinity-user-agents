package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

class Migration23To24 : Migration(23, 24) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL(
            """
            CREATE TABLE IF NOT EXISTS `products_new` (
                `id` TEXT NOT NULL,
                `name` TEXT NOT NULL,
                `scriptUrl` TEXT NOT NULL,
                `appUrl` TEXT NOT NULL,
                PRIMARY KEY(`id`)
            )
            """.trimIndent()
        )

        db.execSQL(
            """
            INSERT INTO `products_new` (`id`, `name`, `scriptUrl`, `appUrl`)
            SELECT `id`, `name`, `url`, '' FROM `products`
            """.trimIndent()
        )

        db.execSQL("DROP TABLE `products`")
        db.execSQL("ALTER TABLE `products_new` RENAME TO `products`")
    }
}
