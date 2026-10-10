package io.paritytech.polkadotapp.feature_videogame_impl.data.collectibles

import android.net.Uri
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isDisabled
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toUri
import io.paritytech.polkadotapp.feature_videogame_api.domain.collectibles.CollectiblesUrlResolver
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import javax.inject.Inject

class RealCollectiblesUrlResolver @Inject constructor(
    private val dotNsTldProvider: DotNsTldProvider,
    private val remoteConfigService: RemoteConfigService,
) : CollectiblesUrlResolver {
    override suspend fun resolveUrl(): Uri? {
        if (!isEnabled()) return null
        val tld = dotNsTldProvider.getTldRetrying()
        return ProductId.fromStoredValue(PRODUCT_LABEL + tld.suffix).toUri()
    }

    private suspend fun isEnabled(): Boolean {
        if (FeatureOption.COLLECTIBLES.isDisabled) return false

        return remoteConfigService.getSyncedBoolean(ENABLED_KEY)
            .logFailure("Reading remote config flag $ENABLED_KEY failed")
            .getOrDefault(false)
    }

    private companion object {
        const val PRODUCT_LABEL = "stash"
        const val ENABLED_KEY = "collectibles_enabled"
    }
}
