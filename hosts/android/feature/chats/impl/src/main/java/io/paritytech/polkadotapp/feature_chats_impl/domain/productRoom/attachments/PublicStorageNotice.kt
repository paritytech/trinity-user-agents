package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments

import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import javax.inject.Inject

/**
 * The notice before the first staged upload of a product: the files go to the public Bulletin chain without
 * encryption (07-LATER §7.2, D27). The host shows it one time for each product.
 */
class PublicStorageNotice @Inject constructor(
    private val preferences: Preferences,
) {
    fun isAccepted(extensionId: ChatExtensionId): Boolean = preferences.getBoolean(key(extensionId), false)

    fun accept(extensionId: ChatExtensionId) = preferences.putBoolean(key(extensionId), true)

    private fun key(extensionId: ChatExtensionId) = "$KEY_PREFIX$extensionId"

    private companion object {
        const val KEY_PREFIX = "chat_public_storage_notice_"
    }
}
