package io.parity.truapi

import android.content.ContentValues
import android.content.Context
import android.database.DatabaseUtils
import android.database.sqlite.SQLiteDatabase
import android.database.sqlite.SQLiteOpenHelper
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.io.File
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import uniffi.truapi_server.HostRejection

/**
 * Durable, device-only core storage, separate from every product's local storage.
 *
 * The opaque slots include Chat identity/history, incoming payment receipts and
 * main-purse allocator/WAL state. A successful write means SQLite committed the
 * encrypted value before the callback returns; the core may then acknowledge it.
 * Never clear this store when clearing a product's browsing data or logging out.
 * One process-owned instance must be shared by all executions of an environment.
 *
 * The Keystore key intentionally remains usable while the screen is locked:
 * process-resident background Chat reception must persist messages and receipts.
 * Mnemonic unlocking and native signing-session retirement are separate gates.
 */
class AndroidHostCoreStorage(context: Context, namespace: String) : HostCoreStorage, AutoCloseable {
    private val namespace = namespace.also {
        require(it.matches(Regex("[a-z0-9][a-z0-9-]{0,62}"))) { "Invalid core storage namespace" }
    }
    private val keyAlias = "io.parity.truapi.core.$namespace"
    private val databaseFile = File(context.noBackupFilesDir, "truapi-core-$namespace.sqlite")
    private val database = object : SQLiteOpenHelper(
        context, databaseFile.absolutePath, 1,
        // OpenParams configures every pooled/reopened connection, unlike a
        // one-time PRAGMA in onConfigure/onOpen.
        SQLiteDatabase.OpenParams.Builder().setSynchronousMode("FULL").build(),
    ) {

        override fun onCreate(db: SQLiteDatabase) {
            db.execSQL("CREATE TABLE core_slots (slot TEXT PRIMARY KEY NOT NULL, ciphertext BLOB NOT NULL)")
        }

        override fun onUpgrade(db: SQLiteDatabase, oldVersion: Int, newVersion: Int) {
            error("Unsupported private core storage version")
        }
    }
    private var encryptionKey: SecretKey? = null

    @Synchronized
    override fun read(key: ByteArray): ByteArray? = storageCall {
        database.readableDatabase.query(
            "core_slots", arrayOf("ciphertext"), "slot = ?", arrayOf(slot(key)),
            null, null, null,
        ).use { cursor ->
            if (!cursor.moveToFirst()) return@storageCall null
            val record = cursor.getBlob(0)
            check(record.size >= 1 + IV_BYTES + TAG_BYTES && record[0] == VERSION) {
                "Invalid private core storage record"
            }
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.DECRYPT_MODE, loadKey(create = false), GCMParameterSpec(128, record, 1, IV_BYTES))
            cipher.updateAAD(key)
            cipher.doFinal(record, 1 + IV_BYTES, record.size - 1 - IV_BYTES)
        }
    }

    @Synchronized
    override fun write(key: ByteArray, value: ByteArray) = storageCall {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, loadKey(create = true))
        cipher.updateAAD(key)
        val record = byteArrayOf(VERSION) + cipher.iv + cipher.doFinal(value)
        durableMutation { db ->
            db.insertWithOnConflict(
                "core_slots", null,
                ContentValues(2).apply {
                    put("slot", slot(key))
                    put("ciphertext", record)
                },
                SQLiteDatabase.CONFLICT_REPLACE,
            ).also { check(it != -1L) { "Private core storage write failed" } }
        }
    }

    @Synchronized
    override fun clear(key: ByteArray) = storageCall {
        durableMutation { db ->
            db.delete("core_slots", "slot = ?", arrayOf(slot(key)))
        }
        Unit
    }

    private inline fun durableMutation(operation: (SQLiteDatabase) -> Unit) {
        val db = database.writableDatabase
        db.beginTransaction()
        try {
            // The transaction pins this thread to the actual writer. Checking
            // outside it could inspect a different pooled (read) connection.
            val synchronous = DatabaseUtils.longForQuery(db, "PRAGMA synchronous", null)
            check(synchronous == 2L || synchronous == 3L) {
                "Private core storage requires FULL or EXTRA durability"
            }
            operation(db)
            db.setTransactionSuccessful()
        } finally {
            // Commit failures propagate through storageCall; never false-ACK.
            db.endTransaction()
        }
    }

    private fun slot(bytes: ByteArray): String {
        require(bytes.isNotEmpty()) { "Empty core storage key" }
        return android.util.Base64.encodeToString(bytes, android.util.Base64.NO_WRAP)
    }

    private fun loadKey(create: Boolean): SecretKey {
        encryptionKey?.let { return it }
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        val existing = store.getKey(keyAlias, null) as? SecretKey
        val resolved = existing ?: run {
            // Missing keys must not silently replace an existing wallet's key.
            check(create && !hasRecords()) { "Private core storage encryption key unavailable" }
            KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").apply {
                init(
                    KeyGenParameterSpec.Builder(
                        keyAlias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
                    ).setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                        .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                        .setKeySize(256)
                        .setUserAuthenticationRequired(false)
                        .build(),
                )
            }.generateKey()
        }
        encryptionKey = resolved
        return resolved
    }

    private fun hasRecords(): Boolean = database.readableDatabase.rawQuery(
        "SELECT 1 FROM core_slots LIMIT 1", null,
    ).use { it.moveToFirst() }

    private inline fun <T> storageCall(operation: () -> T): T = try {
        operation()
    } catch (error: Exception) {
        // Do not expose key material, ciphertext or platform exception details.
        throw HostRejection.Rejected("Private host storage unavailable")
    }

    @Synchronized
    override fun close() {
        database.close()
        encryptionKey = null
    }

    private companion object {
        const val IV_BYTES = 12
        const val TAG_BYTES = 16
        const val VERSION: Byte = 1
    }
}
