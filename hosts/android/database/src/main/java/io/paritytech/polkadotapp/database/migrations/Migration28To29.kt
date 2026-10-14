package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

class Migration28To29 : Migration(28, 29) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE contacts ADD COLUMN isBlocked INTEGER NOT NULL DEFAULT 0")
    }
}
