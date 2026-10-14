package io.paritytech.polkadotapp.feature_products_impl.data.storage

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import javax.inject.Inject
import javax.inject.Singleton

interface ContainerScriptProvider {
    suspend fun loadContainerScript(): String

    fun loadBridgeLibrary(): String
}

@Singleton
class AssetContainerScriptProvider @Inject constructor(
    @param:ApplicationContext private val context: Context
) : ContainerScriptProvider {
    override suspend fun loadContainerScript(): String {
        return withContext(Dispatchers.IO) {
            context.assets.open("container.js").bufferedReader().readText()
        }
    }

    override fun loadBridgeLibrary(): String {
        return """
            window.NativeBridge = {
                _callbacks: {},
                _dispatchEvent: function(id, payload) {
                    var cb = this._callbacks[id];
                    if (cb) cb(payload);
                }
            };
            console.log('NativeBridge initialized');
        """.trimIndent()
    }
}
