package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

class Migration48To49 : Migration(48, 49) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("ALTER TABLE file_downloads ADD COLUMN payloadKind TEXT NOT NULL DEFAULT 'UNRESOLVED'")
        db.execSQL("UPDATE file_downloads SET payloadKind = 'CHUNKED' WHERE chunkHashes IS NOT NULL AND chunkHashes != ''")

        for (table in listOf("file_downloads", "file_uploads")) {
            db.execSQL("ALTER TABLE $table ADD COLUMN attemptCount INTEGER NOT NULL DEFAULT 0")
            db.execSQL("ALTER TABLE $table ADD COLUMN firstFailureAt INTEGER")
            db.execSQL("ALTER TABLE $table ADD COLUMN nextAttemptAt INTEGER")
        }
    }
}
