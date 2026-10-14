package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

class Migration10To11 : Migration(10, 11) {
    companion object {
        // Wallet account
        private const val DEFAULT_OUR_META_ACCOUNT_ID = 1

        // Wallet chat derivation
        private const val DEFAULT_SHARED_SECRET_DERIVATION_PATH = "//wallet//chat"
    }

    override fun migrate(db: SupportSQLiteDatabase) {
        addNewFieldsToContacts(db)
        createGamePlayersTable(db)
    }

    private fun addNewFieldsToContacts(db: SupportSQLiteDatabase) {
        db.execSQL(
            """
            CREATE TABLE IF NOT EXISTS contacts_new (
                accountId BLOB NOT NULL,
                username TEXT,
                chatKey BLOB NOT NULL,
                pin TEXT,
                pushId BLOB,
                pushToken BLOB,
                lastSharedPushToken TEXT,
                operatingSystem TEXT,
                isPeerLeft INTEGER NOT NULL DEFAULT 0,
                ourMetaAccountId INTEGER NOT NULL,
                sharedSecretDerivationPath TEXT NOT NULL,
                avatar TEXT,
                PRIMARY KEY(accountId)
            )
            """.trimIndent()
        )

        db.execSQL(
            """
            INSERT INTO contacts_new (
                accountId, username, chatKey, pin, pushId, pushToken,
                lastSharedPushToken, operatingSystem, isPeerLeft,
                ourMetaAccountId, sharedSecretDerivationPath, avatar
            )
            SELECT
                accountId, username, chatKey, pin, pushId, pushToken,
                lastSharedPushToken, operatingSystem, isPeerLeft,
                $DEFAULT_OUR_META_ACCOUNT_ID, '$DEFAULT_SHARED_SECRET_DERIVATION_PATH', null
            FROM contacts
            """.trimIndent()
        )

        db.execSQL("DROP TABLE contacts")
        db.execSQL("ALTER TABLE contacts_new RENAME TO contacts")
    }

    private fun createGamePlayersTable(db: SupportSQLiteDatabase) {
        db.execSQL(
            """
            CREATE TABLE IF NOT EXISTS game_players (
                gameIndex INTEGER NOT NULL,
                accountId BLOB NOT NULL,
                PRIMARY KEY(gameIndex, accountId)
            )
            """.trimIndent()
        )
    }
}
