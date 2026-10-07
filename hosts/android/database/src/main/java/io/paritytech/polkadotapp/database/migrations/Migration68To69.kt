package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

/**
 * Pocket faces are kept in the core's encoding of the tree rather than as JSON of the host's own
 * widget model, which could not be read back as the tree that was drawn.
 *
 * The old rows cannot be converted, so they are dropped. Each card keeps its place in the Pocket and
 * shows its bundled face, if it has one, until its product draws again.
 */
class Migration68To69 : Migration(68, 69) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL("DROP TABLE `pocket_card_faces`")
        db.execSQL(
            "CREATE TABLE IF NOT EXISTS `pocket_card_faces` " +
                "(`productId` TEXT NOT NULL, `cardId` TEXT NOT NULL, `face` BLOB NOT NULL, PRIMARY KEY(`productId`, `cardId`))"
        )
    }
}
