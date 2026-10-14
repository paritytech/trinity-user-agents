package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.AutoMigrationSpec
import androidx.sqlite.db.SupportSQLiteDatabase

class Migration30To31Spec : AutoMigrationSpec {
    override fun onPostMigrate(db: SupportSQLiteDatabase) {
        db.execSQL("UPDATE chat_messages SET type = 'RICH_TEXT' WHERE type IN ('MEDIA', 'FILE')")
    }
}
