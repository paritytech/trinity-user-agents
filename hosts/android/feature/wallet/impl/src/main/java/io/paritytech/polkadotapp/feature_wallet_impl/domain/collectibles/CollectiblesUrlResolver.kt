package io.paritytech.polkadotapp.feature_wallet_impl.domain.collectibles

import android.net.Uri
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isDisabled
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_products_api.model.toUri
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import uniffi.truapi.productIdFromName
import javax.inject.Inject

class CollectiblesUrlResolver @Inject constructor(
    private val dotNsTldProvider: DotNsTldProvider,
    private val remoteConfigService: RemoteConfigService,
) {
    suspend fun resolveUrl(): Result<Uri?> = runCancellableCatching {
        if (!isEnabled()) return@runCancellableCatching null
        val tld = dotNsTldProvider.getTldRetrying()
        productIdFromName(PRODUCT_LABEL, tld.value).toUri()
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
