package io.paritytech.polkadotapp.database.migrations

import androidx.room.Room
import androidx.room.testing.MigrationTestHelper
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.paritytech.polkadotapp.common.data.storage.preferences.Editor
import io.paritytech.polkadotapp.common.data.storage.preferences.InitialValueProducer
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.database.AppDatabase
import io.paritytech.polkadotapp.database.AppDatabase.Companion.addAppMigrations
import io.paritytech.polkadotapp.feature_chats_impl.data.migrations.createChatMessageMigration1to2
import io.paritytech.polkadotapp.feature_chats_impl.data.migrations.createChatMessageMigration2to3
import io.paritytech.polkadotapp.feature_chats_impl.data.migrations.createChatMessageMigration3to4
import io.paritytech.polkadotapp.feature_chats_impl.data.migrations.createChatMessageMigration4to5
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class MigrationTest {

    @get:Rule
    val helper = MigrationTestHelper(
        InstrumentationRegistry.getInstrumentation(),
        AppDatabase::class.java
    )

    @Test
    fun migrateAll() {
        helper.createDatabase(TEST_DB, 1).close()

        Room.databaseBuilder(
            InstrumentationRegistry.getInstrumentation().targetContext,
            AppDatabase::class.java,
            TEST_DB
        )
            .addAppMigrations(
                NoOpPreferences(),
                setOf(createChatMessageMigration1to2(), createChatMessageMigration2to3(), createChatMessageMigration3to4(), createChatMessageMigration4to5())
            )
            .build()
            .apply {
                openHelper.writableDatabase.close()
                close()
            }
    }

    companion object {
        private const val TEST_DB = "migration-test"
    }
}

private class NoOpPreferences : Preferences {
    override fun contains(field: String) = false
    override fun putString(field: String, value: String?) = Unit
    override fun getString(field: String, defaultValue: String) = defaultValue
    override fun getString(field: String): String? = null
    override fun putBoolean(field: String, value: Boolean) = Unit
    override fun getBoolean(field: String, defaultValue: Boolean) = defaultValue
    override fun putInt(field: String, value: Int) = Unit
    override fun getInt(field: String, defaultValue: Int) = defaultValue
    override fun putLong(field: String, value: Long) = Unit
    override fun getLong(field: String, defaultValue: Long) = defaultValue
    override fun putStringSet(field: String, value: Set<String>) = Unit
    override fun getStringSet(field: String): Set<String> = emptySet()
    override fun removeField(field: String) = Unit
    override fun stringFlow(field: String, initialValueProducer: InitialValueProducer<String>?): Flow<String?> = emptyFlow()
    override fun booleanFlow(field: String, defaultValue: Boolean): Flow<Boolean> = emptyFlow()
    override fun longFlow(field: String, defaultValue: Long): Flow<Long> = emptyFlow()
    override fun intFlow(field: String, defaultValue: Int): Flow<Int> = emptyFlow()
    override fun stringSetFlow(field: String): Flow<Set<String>> = emptyFlow()
    override fun keyFlow(key: String): Flow<String> = emptyFlow()
    override fun keysFlow(vararg keys: String): Flow<List<String>> = emptyFlow()
    override fun edit(): Editor = NoOpEditor()
}

private class NoOpEditor : Editor {
    override fun putString(field: String, value: String?) = Unit
    override fun putBoolean(field: String, value: Boolean) = Unit
    override fun putInt(field: String, value: Int) = Unit
    override fun putLong(field: String, value: Long) = Unit
    override fun remove(field: String) = Unit
    override fun apply() = Unit
}
