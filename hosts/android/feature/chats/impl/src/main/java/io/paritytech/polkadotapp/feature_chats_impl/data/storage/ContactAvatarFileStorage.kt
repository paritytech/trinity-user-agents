package io.paritytech.polkadotapp.feature_chats_impl.data.storage

import io.paritytech.polkadotapp.common.data.storage.file.FileProvider
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_chats_api.domain.profile.ResolvedContactAvatar
import kotlinx.coroutines.withContext
import javax.inject.Inject

class ContactAvatarFileStorage @Inject constructor(
    private val fileProvider: FileProvider,
    private val coroutineDispatchers: CoroutineDispatchers,
) {
    /** The `content://` URI the written image is served under, as the contact row stores it. */
    suspend fun write(accountId: AccountId, avatar: ResolvedContactAvatar): String = withContext(coroutineDispatchers.io) {
        val file = fileProvider.getFileInScopedStorage("$DIRECTORY/${accountId.fileStem()}-${avatar.cid}.${avatar.format.extension}")
        file.parentFile?.mkdirs()
        file.writeBytes(avatar.bytes)
        fileProvider.uriOf(file).toString()
    }

    suspend fun delete(accountId: AccountId, keepCid: String?) = withContext(coroutineDispatchers.io) {
        val stem = "${accountId.fileStem()}-"
        val kept = keepCid?.let { "$stem$it." }
        fileProvider.getFileInScopedStorage(DIRECTORY)
            .listFiles { file -> file.name.startsWith(stem) && (kept == null || !file.name.startsWith(kept)) }
            ?.forEach { it.delete() }
        Unit
    }

    private fun AccountId.fileStem(): String = value.toHexString()

    companion object {
        private const val DIRECTORY = "avatars"
    }
}
