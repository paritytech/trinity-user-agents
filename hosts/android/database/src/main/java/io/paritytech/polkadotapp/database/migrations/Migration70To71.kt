package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

/** Keeps a provider's own failure message with the funding record, so the history can say why. */
class Migration70To71 : Migration(70, 71) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE `product_funding_records` ADD COLUMN `failureMessage` TEXT")
    }
}
