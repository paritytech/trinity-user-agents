package io.paritytech.polkadotapp.feature_videogame_impl.data.collectibles

import android.net.Uri
import androidx.core.net.toUri
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isDisabled
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_videogame_api.domain.collectibles.CollectiblesUrlResolver
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import javax.inject.Inject

class RealCollectiblesUrlResolver @Inject constructor(
    private val dotNsResolver: DotNsResolver,
    private val dotNsTldProvider: DotNsTldProvider,
    private val remoteConfigService: RemoteConfigService,
) : CollectiblesUrlResolver {
    override suspend fun resolveUrl(): Uri? {
        if (!isEnabled()) return null
        return resolveDotNs()
    }

    private suspend fun isEnabled(): Boolean {
        if (FeatureOption.COLLECTIBLES.isDisabled) return false

        return remoteConfigService.getSyncedBoolean(ENABLED_KEY)
            .logFailure("Reading remote config flag $ENABLED_KEY failed")
            .getOrDefault(false)
    }

    private suspend fun resolveDotNs(): Uri? {
        val host = DOT_NS_LABEL + dotNsTldProvider.getTldRetrying().suffix
        return dotNsResolver.resolveToLocalUri(host)
            .map { "https://$host/".toUri() }
            .logFailure("DotNs resolution failed for $host")
            .getOrNull()
    }

    private companion object {
        const val DOT_NS_LABEL = "stash"
        const val ENABLED_KEY = "collectibles_enabled"
    }
}
