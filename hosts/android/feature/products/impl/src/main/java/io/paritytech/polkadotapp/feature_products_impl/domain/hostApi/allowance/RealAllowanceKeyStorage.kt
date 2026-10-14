package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.allowance

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.SlotAccountKey
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import kotlinx.coroutines.withContext
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class RealAllowanceKeyStorage @Inject constructor(
    private val encryptedPreferences: EncryptedPreferences,
    private val dispatchers: CoroutineDispatchers,
) : AllowanceKeyStorage {
    override suspend fun get(productId: ProductId, kind: AllowanceResourceKind): SlotAccountKey? = withContext(dispatchers.io) {
        encryptedPreferences.getDecryptedString(prefsKey(productId, kind))
            ?.let { hex -> SlotAccountKey.fromBytes(DataByteArray(hex.fromHex())).getOrNull() }
    }

    override suspend fun put(productId: ProductId, kind: AllowanceResourceKind, key: SlotAccountKey) = withContext(dispatchers.io) {
        encryptedPreferences.putEncryptedString(
            field = prefsKey(productId, kind),
            value = key.bytes.value.toHexString(),
        )
    }

    private fun prefsKey(productId: ProductId, kind: AllowanceResourceKind): String {
        return "ALLOWANCE_KEY:${productId.value}:${kind.name}"
    }
}
