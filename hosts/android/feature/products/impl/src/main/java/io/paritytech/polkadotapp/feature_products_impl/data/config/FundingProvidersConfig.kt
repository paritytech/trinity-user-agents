package io.paritytech.polkadotapp.feature_products_impl.data.config

import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import io.paritytech.polkadotapp.tools_remoteconfig_api.getSyncedJsonObject
import timber.log.Timber
import uniffi.truapi.ProductId
import javax.inject.Inject

/** The published funding providers the host lists to the core; the core offers no others. */
interface FundingProvidersConfig {
    /** In configured order, without duplicates. Waits for the first Remote Config sync. */
    suspend fun getFundingProviderIds(): Result<List<ProductId>>
}

internal class RemoteConfigFundingProvidersConfig @Inject constructor(
    private val remoteConfigService: RemoteConfigService,
) : FundingProvidersConfig {
    override suspend fun getFundingProviderIds(): Result<List<ProductId>> {
        return remoteConfigService.getSyncedJsonObject<FundingConfigRemote>(CONFIG_KEY)
            .mapCatching { it.toProviderIds() }
    }

    // Gson bypasses Kotlin nullability, so a missing `providers` arrives as null, and so can an element.
    private fun FundingConfigRemote.toProviderIds(): List<ProductId> {
        return providers.orEmpty()
            .mapNotNull { name ->
                name?.let(ProductId::parse)
                    ?.onFailure { Timber.w(it, "Skipping funding provider %s in $CONFIG_KEY", name) }
                    ?.getOrNull()
            }
            .distinct()
    }

    private class FundingConfigRemote(
        val providers: List<String?>?,
    )

    private companion object {
        const val CONFIG_KEY = "funding_config"
    }
}
