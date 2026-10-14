package io.paritytech.polkadotapp.tools_backup_impl.data.store.key

import com.google.api.services.drive.DriveScopes
import com.google.firebase.Firebase
import com.google.firebase.firestore.DocumentSnapshot
import com.google.firebase.firestore.firestore
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.tools_auth_api.FirebaseAuthManager
import io.paritytech.polkadotapp.tools_backup_impl.BuildConfig
import io.paritytech.polkadotapp.tools_backup_impl.data.model.EncryptionKey
import io.paritytech.polkadotapp.tools_backup_impl.data.model.toHex
import io.paritytech.polkadotapp.tools_common.executeSuspend
import javax.inject.Inject

interface BackupEncryptionKeyStore {
    suspend fun store(key: EncryptionKey): Result<Unit>

    suspend fun read(): Result<EncryptionKey?>
}

private const val COLLECTION_NAME = "users" + BuildConfig.BACKUP_KEY_SUFFIX
private const val KEY_PROPERTY_NAME = "key"

class PasskeysBackupEncryptionKeyStore @Inject constructor(
    private val fbAuthManager: FirebaseAuthManager,
) : BackupEncryptionKeyStore {
    private val db = Firebase.firestore(BuildConfig.FIRESTORE_DATABASE_ID)

    override suspend fun store(key: EncryptionKey): Result<Unit> {
        return fbAuthManager.authenticate(driveScope())
            .flatMap { storeKey(key, it) }
            .coerceToUnit()
    }

    override suspend fun read(): Result<EncryptionKey?> {
        return fbAuthManager.authenticate(driveScope())
            .flatMap { readKey(it) }
    }

    private suspend fun readKey(uid: String): Result<EncryptionKey?> {
        return db.collection(COLLECTION_NAME)
            .document(uid)
            .get()
            .executeSuspend()
            .mapCatching { it?.getKey() }
    }

    private suspend fun storeKey(key: EncryptionKey, uid: String): Result<Unit> {
        val newRecord = hashMapOf(KEY_PROPERTY_NAME to key.toHex())
        return db.collection(COLLECTION_NAME)
            .document(uid)
            .set(newRecord)
            .executeSuspend()
            .coerceToUnit()
    }

    // we are using driveScope here to avoid additional permissions pop up when google drive auth check starts
    private fun driveScope(): String = DriveScopes.DRIVE_FILE

    private fun DocumentSnapshot.getKey() =
        (data?.get(KEY_PROPERTY_NAME) as String?)?.let { EncryptionKey(it.fromHex()) }
}
