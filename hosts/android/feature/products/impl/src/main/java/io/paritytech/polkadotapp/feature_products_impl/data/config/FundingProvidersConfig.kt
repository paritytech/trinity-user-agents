package io.paritytech.polkadotapp.feature_products_impl.data.config

import com.google.gson.Gson
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.mapNotNull
import javax.inject.Inject

/**
 * The published funding providers Remote Config lists, by name without a TLD; the core reads each on the network it is
 * connected to and offers no others.
 */
interface FundingProvidersConfig {
    /** The names as configured, after the first Remote Config sync and again on every update. */
    fun publishedNames(): Flow<List<String>>
}

internal class RemoteConfigFundingProvidersConfig @Inject constructor(
    private val remoteConfigService: RemoteConfigService,
) : FundingProvidersConfig {
    private val gson = Gson()

    // A value that does not parse, which Gson may read as no object at all, keeps the list the core had.
    override fun publishedNames(): Flow<List<String>> = remoteConfigService.observeString(CONFIG_KEY).mapNotNull { raw ->
        val config = runCatching { gson.fromJson(raw, FundingConfigRemote::class.java) }
            .logFailure("Unreadable $CONFIG_KEY")
            .getOrNull()
            ?: return@mapNotNull null
        // Gson bypasses Kotlin nullability, so a missing `providers` arrives as null, and so can an element.
        config.providers.orEmpty().filterNotNull()
    }

    private class FundingConfigRemote(
        val providers: List<String?>?,
    )

    private companion object {
        const val CONFIG_KEY = "funding_config"
    }
}
