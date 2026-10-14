package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences

private const val POLKADOT_PEER_BOT_ID = "PolkadotPeerBot"
private const val WELCOME_MESSAGE_KEY = "RealChatMessageProcessingRepository.WELCOME_MESSAGE_KEY"

class Migration3To4(private val preferences: Preferences) : Migration(3, 4) {
    override fun migrate(db: SupportSQLiteDatabase) {
        // Delete all messages from PolkadotPeerBot
        // originkey stores the middleware id as UTF-8 encoded bytes
        val botIdBytes = POLKADOT_PEER_BOT_ID.encodeToByteArray()
        db.execSQL(
            "DELETE FROM chat_messages WHERE origintype = 'MIDDLEWARE' AND originkey = ?",
            arrayOf(botIdBytes)
        )

        clearPolkadotPeerBotWelcomeMessagePreference()
    }

    private fun clearPolkadotPeerBotWelcomeMessagePreference() {
        val key = "$WELCOME_MESSAGE_KEY:$POLKADOT_PEER_BOT_ID"
        preferences.removeField(key)
    }
}
