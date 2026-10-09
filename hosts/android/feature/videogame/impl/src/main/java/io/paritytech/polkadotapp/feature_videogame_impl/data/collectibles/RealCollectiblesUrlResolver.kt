package io.paritytech.polkadotapp.feature_videogame_impl.data.collectibles

import android.net.Uri
import androidx.core.net.toUri
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flatRecover
import io.paritytech.polkadotapp.common.utils.isDisabled
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_videogame_api.domain.collectibles.CollectiblesUrlResolver
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import kotlinx.serialization.json.Json
import javax.inject.Inject

class RealCollectiblesUrlResolver @Inject constructor(
    private val dotNsResolver: DotNsResolver,
    private val dotNsTldProvider: DotNsTldProvider,
    private val remoteConfigService: RemoteConfigService,
) : CollectiblesUrlResolver {
    override suspend fun resolveUrl(): Uri? {
        if (!isEnabled()) return null
        return resolveDotNs()
            .flatRecover { resolveRemoteConfig() }
            .getOrNull()
    }

    private suspend fun isEnabled(): Boolean {
        if (FeatureOption.COLLECTIBLES.isDisabled) return false

        return remoteConfigService.getSyncedBoolean(ENABLED_KEY)
            .logFailure("Reading remote config flag $ENABLED_KEY failed")
            .getOrDefault(false)
    }

    private suspend fun resolveDotNs(): Result<Uri> {
        return dotNsTldProvider.getTld()
            .map { tld -> DOT_NS_LABEL + tld.suffix }
            .flatMap { host -> dotNsResolver.resolveToLocalUri(host).map { "https://$host/".toUri() } }
            .logFailure("DotNs resolution failed for $DOT_NS_LABEL")
    }

    private suspend fun resolveRemoteConfig(): Result<Uri> {
        return remoteConfigService.getSyncedString(REMOTE_CONFIG_KEY)
            // Firebase Remote Config values are JSON-encoded for cross-platform parity
            // with iOS, which reads them via Decodable. The stored value is a JSON
            // string literal (with surrounding quotes), so we unwrap it before parsing.
            .mapCatching { raw ->
                val decoded = Json.decodeFromString<String>(raw)
                require(decoded.isNotBlank()) { "decoded URL is blank" }
                decoded.toUri()
            }
            .logFailure("Firebase fallback URL parsing failed for $REMOTE_CONFIG_KEY")
    }

    private companion object {
        const val DOT_NS_LABEL = "stash"
        const val REMOTE_CONFIG_KEY = "collectibles_fallback_url"
        const val ENABLED_KEY = "collectibles_enabled"
    }
}
