package io.paritytech.polkadotapp.database.migrations

import androidx.room.DeleteColumn
import androidx.room.DeleteTable
import androidx.room.migration.AutoMigrationSpec
import androidx.sqlite.db.SupportSQLiteDatabase

@DeleteTable.Entries(
    DeleteTable(tableName = "chat_bots"),
)
@DeleteColumn.Entries(
    DeleteColumn(tableName = "chat_room_declarations", columnName = "avatar"),
    DeleteColumn(tableName = "chat_room_declarations", columnName = "mutedUntil"),
    DeleteColumn(tableName = "chat_room_declarations", columnName = "archived"),
    DeleteColumn(tableName = "chat_messages", columnName = "originParticipantId"),
)
class Migration73To74Spec : AutoMigrationSpec {
    // A product can no longer delete a message, so the rows that marked a deleted one go. A stored preview has a new
    // layout without a version prefix, so the old ones go too, and each product declares its previews again.
    override fun onPostMigrate(db: SupportSQLiteDatabase) {
        db.execSQL("DELETE FROM chat_messages WHERE type = 'DELETED'")
        db.execSQL("UPDATE chat_room_declarations SET preview = NULL")
    }
}
