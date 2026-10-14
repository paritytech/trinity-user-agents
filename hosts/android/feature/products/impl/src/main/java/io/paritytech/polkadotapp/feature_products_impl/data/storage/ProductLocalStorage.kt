package io.paritytech.polkadotapp.feature_products_impl.data.storage

import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

private const val PREFS_KEY_PREFIX = "ProductStorage"

interface ProductLocalStorage {
    fun read(productId: ProductId, key: String): String?
    fun write(productId: ProductId, key: String, value: String)
    fun clear(productId: ProductId, key: String)
    fun observe(productId: ProductId, key: String): Flow<String?>
}

class RealProductLocalStorage @Inject constructor(
    private val preferences: Preferences
) : ProductLocalStorage {
    override fun read(productId: ProductId, key: String): String? {
        return preferences.getString(createPrefsKey(productId, key))
    }

    override fun write(productId: ProductId, key: String, value: String) {
        preferences.putString(createPrefsKey(productId, key), value)
    }

    override fun clear(productId: ProductId, key: String) {
        preferences.removeField(createPrefsKey(productId, key))
    }

    override fun observe(productId: ProductId, key: String): Flow<String?> {
        return preferences.stringFlow(createPrefsKey(productId, key))
    }

    private fun createPrefsKey(productId: ProductId, key: String): String {
        return "$PREFS_KEY_PREFIX.${productId.value}.$key"
    }
}
