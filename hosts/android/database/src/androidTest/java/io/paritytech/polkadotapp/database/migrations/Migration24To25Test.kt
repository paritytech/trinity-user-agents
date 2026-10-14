package io.paritytech.polkadotapp.database.migrations

import android.content.ContentValues
import android.database.sqlite.SQLiteDatabase
import androidx.sqlite.db.SupportSQLiteDatabase
import androidx.room.testing.MigrationTestHelper
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.paritytech.polkadotapp.database.AppDatabase
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class Migration24To25Test {

    @get:Rule
    val helper = MigrationTestHelper(
        InstrumentationRegistry.getInstrumentation(),
        AppDatabase::class.java
    )

    private lateinit var db: SupportSQLiteDatabase
    private lateinit var migratedDb: SupportSQLiteDatabase

    @Before
    fun setup() {
        db = helper.createDatabase(TEST_DB, 24)
    }

    @After
    fun tearDown() {
        if (::migratedDb.isInitialized) {
            migratedDb.close()
        }
    }

    @Test
    fun prefixesAreRewrittenInChatMessages() {
        db.insertChatMessage(id = "msg1", chatId = "ChatBot:TattooBot".toByteArray())

        val migrated = migrate()

        val cursor = migrated.query("SELECT chatId FROM chat_messages WHERE id = 'msg1'")
        assertTrue(cursor.moveToFirst())
        assertEquals("ChatExtension:TattooBot", cursor.getBlob(0).decodeToString())
        cursor.close()
    }

    @Test
    fun prefixesAreRewrittenInChatMessageProcessing() {
        val oldChatId = "ChatBot:SampleBot".toByteArray()
        db.insertChatMessage(id = "msg1", chatId = oldChatId)
        db.insertChatMessageProcessing(chatId = oldChatId, messageId = "msg1", middlewareId = "SampleBot")

        val migrated = migrate()

        val cursor = migrated.query("SELECT chatId FROM chat_message_processing WHERE messageId = 'msg1'")
        assertTrue(cursor.moveToFirst())
        assertEquals("ChatExtension:SampleBot", cursor.getBlob(0).decodeToString())
        cursor.close()
    }

    @Test
    fun prefixesAreRewrittenInChatMessageReactions() {
        val oldChatId = "ChatBot:WeeklyGameBot".toByteArray()
        db.insertChatMessage(id = "msg1", chatId = oldChatId)
        db.insertChatMessageReaction(chatId = oldChatId, messageId = "msg1")

        val migrated = migrate()

        val cursor = migrated.query("SELECT chatId FROM chat_message_reactions WHERE messageId = 'msg1'")
        assertTrue(cursor.moveToFirst())
        assertEquals("ChatExtension:WeeklyGameBot", cursor.getBlob(0).decodeToString())
        cursor.close()
    }

    @Test
    fun prefixesAreRewrittenInMessageRevisions() {
        val oldChatId = "ChatBot:MobRuleBot".toByteArray()
        db.insertChatMessage(id = "msg1", chatId = oldChatId)
        db.insertMessageRevision(chatId = oldChatId, messageId = "msg1")

        val migrated = migrate()

        val cursor = migrated.query("SELECT chatId FROM message_revisions WHERE messageId = 'msg1'")
        assertTrue(cursor.moveToFirst())
        assertEquals("ChatExtension:MobRuleBot", cursor.getBlob(0).decodeToString())
        cursor.close()
    }

    @Test
    fun roomsAreCreatedForActiveBots() {
        db.insertBotState(middlewareId = "TattooBot", isActive = true)
        db.insertBotState(middlewareId = "InactiveBot", isActive = false)

        val migrated = migrate()

        val cursor = migrated.query("SELECT id FROM chat_rooms")
        val roomIds = mutableListOf<String>()
        while (cursor.moveToNext()) {
            roomIds.add(cursor.getBlob(0).decodeToString())
        }
        cursor.close()

        assertTrue("Active bot should have a room", roomIds.contains("ChatExtension:TattooBot"))
        assertTrue("Inactive bot should NOT have a room", !roomIds.contains("ChatExtension:InactiveBot"))
    }

    @Test
    fun roomsAreCreatedForContacts() {
        val contactAccountId = ByteArray(32) { it.toByte() }
        db.insertContact(accountId = contactAccountId)

        val migrated = migrate()

        val cursor = migrated.query("SELECT id FROM chat_rooms")
        assertTrue(cursor.moveToFirst())
        val roomId = cursor.getBlob(0)
        cursor.close()

        assertNotNull("Contact should have a room", roomId)
        assertTrue("Room ID should match contact accountId", contactAccountId.contentEquals(roomId))
    }

    @Test
    fun contactChatIdsAreNotAffectedByPrefixRewrite() {
        val contactChatId = ByteArray(32) { it.toByte() }
        db.insertChatMessage(id = "contact_msg", chatId = contactChatId)

        val migrated = migrate()

        val cursor = migrated.query("SELECT chatId FROM chat_messages WHERE id = 'contact_msg'")
        assertTrue(cursor.moveToFirst())
        val migratedChatId = cursor.getBlob(0)
        cursor.close()

        assertTrue("Contact chatId should remain unchanged", contactChatId.contentEquals(migratedChatId))
    }

    // --- Helper insert methods ---

    private fun SupportSQLiteDatabase.insertChatMessage(id: String, chatId: ByteArray) {
        val values = ContentValues().apply {
            put("id", id)
            put("chatId", chatId)
            put("timestamp", System.currentTimeMillis())
            put("origintype", "USER")
            put("status", "NEW")
            put("type", "TEXT")
            put("searchableContent", "test")
            put("content", ByteArray(0))
            put("isInternal", false)
        }
        insert("chat_messages", SQLiteDatabase.CONFLICT_NONE, values)
    }

    private fun SupportSQLiteDatabase.insertChatMessageProcessing(chatId: ByteArray, messageId: String, middlewareId: String) {
        val values = ContentValues().apply {
            put("chatId", chatId)
            put("messageId", messageId)
            put("middlewareId", middlewareId)
        }
        insert("chat_message_processing", SQLiteDatabase.CONFLICT_NONE, values)
    }

    private fun SupportSQLiteDatabase.insertChatMessageReaction(chatId: ByteArray, messageId: String) {
        val values = ContentValues().apply {
            put("messageId", messageId)
            put("emoji", "👍")
            put("origintype", "USER")
            put("chatId", chatId)
            put("timestamp", System.currentTimeMillis())
        }
        insert("chat_message_reactions", SQLiteDatabase.CONFLICT_NONE, values)
    }

    private fun SupportSQLiteDatabase.insertMessageRevision(chatId: ByteArray, messageId: String) {
        val values = ContentValues().apply {
            put("messageId", messageId)
            put("type", "TEXT")
            put("content", ByteArray(0))
            put("chatId", chatId)
            put("timestamp", System.currentTimeMillis())
        }
        insert("message_revisions", SQLiteDatabase.CONFLICT_NONE, values)
    }

    private fun SupportSQLiteDatabase.insertBotState(middlewareId: String, isActive: Boolean) {
        val values = ContentValues().apply {
            put("middlewareId", middlewareId)
            put("isActive", isActive)
        }
        insert("chat_bot_state", SQLiteDatabase.CONFLICT_NONE, values)
    }

    private fun SupportSQLiteDatabase.insertContact(accountId: ByteArray) {
        val values = ContentValues().apply {
            put("accountId", accountId)
            put("chatKey", ByteArray(32))
            put("ourMetaAccountId", 1L)
            put("sharedSecretDerivationPath", "//test")
            put("isPeerLeft", false)
            put("origin", "contactChat")
        }
        insert("contacts", SQLiteDatabase.CONFLICT_NONE, values)
    }

    companion object {
        private const val TEST_DB = "migration-22-to-23-test"
    }

    private fun migrate(): SupportSQLiteDatabase {
        db.close()
        migratedDb = helper.runMigrationsAndValidate(TEST_DB, 25, true, Migration24To25())
        return migratedDb
    }
}
