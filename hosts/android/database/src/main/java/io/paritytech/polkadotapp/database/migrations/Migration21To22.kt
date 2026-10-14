package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

class Migration21To22 : Migration(21, 22) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL(
            """
            UPDATE chat_coinage_transfer_detection
            SET status = 'FAILED_DETECTION'
            WHERE status = 'FAILED'
            """.trimIndent()
        )
    }
}
