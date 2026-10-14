package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase

class Migration16To17 : Migration(16, 17) {
    override fun migrate(db: SupportSQLiteDatabase) {
        db.execSQL(
            """
            CREATE TABLE IF NOT EXISTS video_game_votes_new (
                accountId BLOB NOT NULL,
                roundIndex INTEGER NOT NULL,
                playerIndex INTEGER NOT NULL,
                vote TEXT NOT NULL,
                gameIndex INTEGER NOT NULL,
                id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL
            )
            """.trimIndent()
        )

        db.execSQL(
            """
            INSERT INTO video_game_votes_new (accountId, roundIndex, playerIndex, vote, gameIndex, id)
            SELECT accountId, roundIndex, playerIndex,
                CASE WHEN isPerson = 1 THEN 'PERSON' ELSE 'NON_PERSON' END,
                gameIndex, id
            FROM video_game_votes
            """.trimIndent()
        )

        db.execSQL("DROP TABLE video_game_votes")
        db.execSQL("ALTER TABLE video_game_votes_new RENAME TO video_game_votes")
    }
}
