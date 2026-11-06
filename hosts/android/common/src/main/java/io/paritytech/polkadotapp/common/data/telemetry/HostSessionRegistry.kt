package io.paritytech.polkadotapp.common.data.telemetry

import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class HostSessionRegistry @Inject constructor() {
    private val sessionIdsByProduct = ConcurrentHashMap<String, String>()

    fun open(productId: String, sessionId: String) {
        sessionIdsByProduct[productId] = sessionId
    }

    fun close(productId: String, sessionId: String) {
        sessionIdsByProduct.remove(productId, sessionId)
    }

    fun sessionIdForProductOrSubname(dotNsName: String): String? = sessionIdsByProduct[dotNsName]
        ?: sessionIdsByProduct.entries
            .filter { (productId, _) -> dotNsName.endsWith(".$productId") }
            .maxByOrNull { (productId, _) -> productId.length }
            ?.value
}
