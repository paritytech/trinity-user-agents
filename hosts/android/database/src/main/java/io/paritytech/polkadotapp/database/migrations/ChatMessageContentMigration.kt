package io.paritytech.polkadotapp.database.migrations

import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.paritytech.polkadotapp.common.BuildConfig
import kotlinx.serialization.KSerializer
import timber.log.Timber

/**
 * A general-purpose migration for transforming SCALE-encoded chat message content.
 *
 * This migration fetches all message contents from the database, decodes them using
 * the provided [oldSerializer], transforms them using the [mapper] function,
 * re-encodes using the provided [newSerializer], and saves back to the database.
 *
 * Only messages where the content actually changed after transformation are updated.
 *
 * ## When to create a migration
 *
 * A new migration is required for every change to `ChatMessageContentLocal` or its nested types
 * (e.g. `RichTextContentLocal`, `AttachmentLocal`, `AttachmentMetaLocal`).
 *
 * ## How to create a migration
 *
 * 1. Copy the current `ChatMessageContentLocal` (and related types) to `data/migrations/schemas/`
 *    as `ContentV{N}` — this freezes the old schema
 * 2. Make your changes to `ChatMessageContentLocal` — it always represents the latest version
 * 3. Create a new migration function in `data/migrations/` that maps `ContentV{N}` → `ChatMessageContentLocal`
 * 4. Increment the database version in `AppDatabase` — each content migration requires its own
 *    unique version range. Room does not support multiple migrations for the same version pair.
 * 5. Register the migration in `ChatContentMigrationsModule` via `@Provides @IntoSet`
 * 6. Add a backward compatibility test in `ChatMessageContentLocalEncodingTest`
 *
 * ## Example
 *
 * ```kotlin
 * fun createMigration2to3(): ChatMessageContentMigration<*, *> = ChatMessageContentMigration(
 *     versionFrom = 27,
 *     versionTo = 28,
 *     oldSerializer = ContentV2.serializer(),
 *     newSerializer = ChatMessageContentLocal.serializer(),
 *     mapper = { old -> ... }
 * )
 * ```
 *
 * @param TOld The type representing the old schema
 * @param TNew The type representing the new schema
 * @param versionFrom The database version to migrate from
 * @param versionTo The database version to migrate to
 * @param oldSerializer The [KSerializer] for decoding the old schema
 * @param newSerializer The [KSerializer] for encoding the new schema
 * @param mapper A function that transforms the old schema type [TOld] to [TNew]
 */
class ChatMessageContentMigration<TOld, TNew>(
    versionFrom: Int,
    versionTo: Int,
    private val oldSerializer: KSerializer<TOld>,
    private val newSerializer: KSerializer<TNew>,
    private val mapper: (TOld) -> TNew
) : Migration(versionFrom, versionTo) {
    override fun migrate(db: SupportSQLiteDatabase) {
        val messageContents = fetchAllMessageContents(db)

        messageContents.forEach { (id, oldContent) ->
            try {
                val decoded = BinaryScale.decodeFromByteArray(oldSerializer, oldContent)
                val mapped = mapper(decoded)
                val newContent = BinaryScale.encodeToByteArray(newSerializer, mapped)

                if (!oldContent.contentEquals(newContent)) {
                    updateMessageContent(db, id, newContent)
                }
            } catch (e: Exception) {
                if (BuildConfig.DEBUG) {
                    throw IllegalStateException(
                        "Chat content migration $startVersion->$endVersion failed for message $id. content=${oldContent.toHexString(withPrefix = true)}",
                        e
                    )
                }

                Timber.e(
                    e,
                    "Chat content migration $startVersion->$endVersion failed for message $id, leaving it unchanged. content=${oldContent.toHexString(withPrefix = true)}"
                )
            }
        }
    }

    private fun fetchAllMessageContents(db: SupportSQLiteDatabase): List<Pair<String, ByteArray>> {
        val results = mutableListOf<Pair<String, ByteArray>>()

        db.query("SELECT id, content FROM chat_messages").use { cursor ->
            while (cursor.moveToNext()) {
                val id = cursor.getString(cursor.getColumnIndexOrThrow("id"))
                val content = cursor.getBlob(cursor.getColumnIndexOrThrow("content"))
                results.add(id to content)
            }
        }

        return results
    }

    private fun updateMessageContent(db: SupportSQLiteDatabase, id: String, content: ByteArray) {
        db.execSQL(
            "UPDATE chat_messages SET content = ? WHERE id = ?",
            arrayOf(content, id)
        )
    }
}
