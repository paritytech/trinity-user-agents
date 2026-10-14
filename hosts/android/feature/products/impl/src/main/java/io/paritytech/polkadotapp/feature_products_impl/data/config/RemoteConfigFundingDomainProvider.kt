package io.paritytech.polkadotapp.feature_products_impl.data.config

import androidx.core.net.toUri
import io.paritytech.polkadotapp.common.data.memory.SingleValueCache
import io.paritytech.polkadotapp.common.data.memory.getCatching
import io.paritytech.polkadotapp.common.utils.Urls
import io.paritytech.polkadotapp.common.utils.combineResults
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.domain.FundingConfig
import io.paritytech.polkadotapp.feature_products_api.domain.FundingDomainProvider
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import io.paritytech.polkadotapp.tools_remoteconfig_api.getSyncedJsonObject
import javax.inject.Inject

internal class RemoteConfigFundingDomainProvider @Inject constructor(
    private val remoteConfigService: RemoteConfigService,
    private val dotNsTldProvider: DotNsTldProvider,
) : FundingDomainProvider {
    // Unwrapping inside the compute keeps a failed read out of the cache; getCatching re-wraps the throw.
    private val fundingConfigCache = SingleValueCache {
        remoteConfigService.getSyncedJsonObject<FundingConfigRemote>(CONFIG_KEY).getOrThrow().toDomain()
    }

    override suspend fun getFundingConfig(): Result<FundingConfig> = fundingConfigCache.getCatching()

    override suspend fun getFundingProductIds(): Result<Set<ProductId>> {
        return getFundingConfig().flatMap { config ->
            dotNsTldProvider.getTld().flatMap { tld ->
                combineResults(config.onrampUrl.toProductId(tld), config.offrampUrl.toProductId(tld)) { onramp, offramp ->
                    setOf(onramp, offramp)
                }
            }
        }
    }

    private fun String.toProductId(tld: DotNsTld): Result<ProductId> {
        return ProductId.fromUrl(Urls.ensureHasProtocolOrHttps(this).toUri(), tld)
    }

    // Gson bypasses Kotlin nullability, so a key missing from the remote JSON arrives as null.
    private fun FundingConfigRemote.toDomain(): FundingConfig {
        return FundingConfig(
            onrampUrl = onrampUrl.requireUrl("onrampUrl"),
            offrampUrl = offrampUrl.requireUrl("offrampUrl"),
        )
    }

    private fun String?.requireUrl(field: String): String {
        return this?.takeIf { it.isNotBlank() } ?: error("Remote Config $CONFIG_KEY carries no $field")
    }

    private class FundingConfigRemote(
        val onrampUrl: String?,
        val offrampUrl: String?,
    )

    private companion object {
        const val CONFIG_KEY = "funding_config"
    }
}
