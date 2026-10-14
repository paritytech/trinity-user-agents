package io.paritytech.polkadotapp.feature_chats_impl.data.hop

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.util.externalApi
import timber.log.Timber
import javax.inject.Inject

class HopNodeUrlProvider @Inject constructor(
    private val chainRegistry: ChainRegistry
) {
    suspend fun availableUrls(): List<String> {
        val bulletIn = chainRegistry.bulletInChain()
        val hopUrls = bulletIn.externalApi<Chain.ExternalApi.Hop>()?.urls.orEmpty()

        if (hopUrls.isNotEmpty()) return hopUrls

        Timber.w("BulletIn chain ${bulletIn.name} has no Hop external API — falling back to its first node url")
        return listOfNotNull(bulletIn.nodes.nodes.firstOrNull()?.unformattedUrl)
    }

    suspend fun pickForSending(): String {
        val urls = availableUrls()
        return urls.randomOrNull() ?: error("No Hop node URLs available for bulletIn chain")
    }

    suspend fun isAllowed(nodeUrl: String): Boolean {
        return availableUrls().any { it == nodeUrl }
    }

    suspend fun allWithSenderPriority(senderNodeUrl: String): List<String> {
        val available = availableUrls()
        return listOf(senderNodeUrl) + available.filter { it != senderNodeUrl }
    }
}
