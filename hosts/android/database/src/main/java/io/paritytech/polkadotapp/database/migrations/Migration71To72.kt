package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

/** Adds the funding history: one record per ended funding session, keyed by its intent. */
class Migration71To72 : Migration(71, 72) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL(
            "CREATE TABLE IF NOT EXISTS `product_funding_records` (" +
                "`intent` TEXT NOT NULL, `direction` TEXT NOT NULL, `rail` TEXT, `asset` TEXT, `providerId` TEXT, " +
                "`requestedPlanks` TEXT, `settledPlanks` TEXT, `outcome` TEXT NOT NULL, `failureCode` TEXT, " +
                "`failureMessage` TEXT, `payout` TEXT, `payoutFailureReason` TEXT, `refundedPlanks` TEXT, " +
                "`transactionId` TEXT, `reference` TEXT, `openedAtMillis` INTEGER NOT NULL, `settledAtMillis` INTEGER NOT NULL, " +
                "PRIMARY KEY(`intent`))"
        )
    }
}
