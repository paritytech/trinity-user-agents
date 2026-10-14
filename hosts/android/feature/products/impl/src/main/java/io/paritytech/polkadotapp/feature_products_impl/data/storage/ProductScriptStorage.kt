package io.paritytech.polkadotapp.feature_products_impl.data.storage

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Storage for product scripts.
 * Manages saving and loading JavaScript scripts to/from internal storage.
 */
interface ProductScriptStorage {
    suspend fun saveScript(productId: ProductId, content: String)

    suspend fun loadScript(productId: ProductId): String?

    suspend fun deleteScript(productId: ProductId)

    fun scriptExists(productId: ProductId): Boolean
}

@Singleton
class InternalProductScriptStorage @Inject constructor(
    @param:ApplicationContext private val context: Context,
) : ProductScriptStorage {
    companion object {
        private const val SCRIPTS_DIR = "product_scripts"
    }

    private val scriptsDir: File
        get() = File(context.filesDir, SCRIPTS_DIR).also { it.mkdirs() }

    override suspend fun saveScript(productId: ProductId, content: String) {
        withContext(Dispatchers.IO) {
            scriptFile(productId).writeText(content)
        }
    }

    override suspend fun loadScript(productId: ProductId): String? {
        return withContext(Dispatchers.IO) {
            val file = scriptFile(productId)
            if (file.exists()) file.readText() else null
        }
    }

    override suspend fun deleteScript(productId: ProductId) {
        withContext(Dispatchers.IO) {
            scriptFile(productId).delete()
        }
    }

    override fun scriptExists(productId: ProductId): Boolean {
        return scriptFile(productId).exists()
    }

    private fun scriptFile(productId: ProductId): File = File(scriptsDir, "${productId.value}.js")
}
