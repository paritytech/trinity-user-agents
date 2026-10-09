package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

class Migration69To70 : Migration(69, 70) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL(
            "CREATE TABLE IF NOT EXISTS `product_funding_records` (" +
                "`intent` TEXT NOT NULL, `direction` TEXT NOT NULL, `rail` TEXT, `asset` TEXT, `providerId` TEXT, " +
                "`requestedPlanks` TEXT, `settledPlanks` TEXT, `outcome` TEXT NOT NULL, `failureCode` TEXT, " +
                "`payout` TEXT, `payoutFailureReason` TEXT, `transactionId` TEXT, `reference` TEXT, " +
                "`openedAtMillis` INTEGER NOT NULL, `settledAtMillis` INTEGER NOT NULL, PRIMARY KEY(`intent`))"
        )
    }
}
