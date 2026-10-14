package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

class Migration68To69 : Migration(68, 69) {
    override fun migrate(db: SupportSQLiteDatabase) {
        // JSON of the old widget model cannot be read back as the core's tree, so the faces go and the
        // cards stay, until their products draw again.
        db.execSQL("DROP TABLE `pocket_card_faces`")
        db.execSQL(
            "CREATE TABLE IF NOT EXISTS `pocket_card_faces` " +
                "(`productId` TEXT NOT NULL, `cardId` TEXT NOT NULL, `face` BLOB NOT NULL, PRIMARY KEY(`productId`, `cardId`))"
        )
    }
}
