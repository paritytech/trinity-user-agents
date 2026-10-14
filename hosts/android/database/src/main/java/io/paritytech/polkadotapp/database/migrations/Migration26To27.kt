package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

class Migration26To27 : Migration(26, 27) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL(
            """
            CREATE TABLE IF NOT EXISTS recycler_vouchers_new (
                ringVrfKeyIndex INTEGER NOT NULL PRIMARY KEY,
                ringVrfPublicKey BLOB NOT NULL,
                recyclerValue INTEGER NOT NULL,
                locationRecyclerIndex INTEGER,
                allocatedAt INTEGER NOT NULL,
                delayUnloadUntil INTEGER NOT NULL,
                ringHasEnoughRingMembersToWithdraw INTEGER NOT NULL,
                usageState TEXT NOT NULL
            )
            """.trimIndent()
        )

        db.execSQL(
            """
            INSERT INTO recycler_vouchers_new (
                ringVrfKeyIndex, ringVrfPublicKey, recyclerValue,
                locationRecyclerIndex,
                allocatedAt, delayUnloadUntil,
                ringHasEnoughRingMembersToWithdraw, usageState
            )
            SELECT
                ringVrfKeyIndex, ringVrfPublicKey, recyclerValue,
                locationRecyclerIndex,
                allocatedAt, delayUnloadUntil,
                ringHasEnoughRingMembersToWithdraw, usageState
            FROM recycler_vouchers
            """.trimIndent()
        )

        db.execSQL("DROP TABLE recycler_vouchers")
        db.execSQL("ALTER TABLE recycler_vouchers_new RENAME TO recycler_vouchers")
    }
}
