package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import javax.inject.Inject

// Where the app kept its device key before the core held one; contacts still address it.
private const val APP_DEVICE_PRIVATE_KEY = "our_device_private_key"

class TrUAPIDeviceKeyHandOver @Inject constructor(
    private val encryptedPreferences: EncryptedPreferences,
) {
    fun keyToHandOver(): ByteArray? = encryptedPreferences.getDecryptedString(APP_DEVICE_PRIVATE_KEY)?.fromHex()

    fun forgetHandedOverKey() {
        encryptedPreferences.removeKey(APP_DEVICE_PRIVATE_KEY)
    }
}
