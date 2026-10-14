package io.paritytech.polkadotapp.feature_xcm_impl.config

import com.google.gson.Gson
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flattenKeys
import io.paritytech.polkadotapp.common.utils.flowOf
import io.paritytech.polkadotapp.common.utils.fromJson
import io.paritytech.polkadotapp.feature_xcm_api.config.XcmConfigRepository
import io.paritytech.polkadotapp.feature_xcm_api.config.model.AssetsXcmConfig
import io.paritytech.polkadotapp.feature_xcm_api.config.model.ChainAssetReserveConfig
import io.paritytech.polkadotapp.feature_xcm_api.config.model.ChainXcmConfig
import io.paritytech.polkadotapp.feature_xcm_api.config.model.GeneralXcmConfig
import io.paritytech.polkadotapp.feature_xcm_api.config.remote.toAbsoluteLocation
import io.paritytech.polkadotapp.feature_xcm_impl.config.api.response.AssetsXcmConfigRemote
import io.paritytech.polkadotapp.feature_xcm_impl.config.api.response.ChainAssetReserveConfigRemote
import io.paritytech.polkadotapp.feature_xcm_impl.config.api.response.ChainXcmConfigRemote
import io.paritytech.polkadotapp.feature_xcm_impl.config.api.response.GeneralXcmConfigRemote
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

class RealXcmConfigRepository @Inject constructor(
    private val gson: Gson,
    private val remoteConfig: RemoteConfigService
) : XcmConfigRepository {
    companion object {
        private const val REMOTE_CONFIG_NAME = "xcm_general_config"
    }

    override suspend fun awaitXcmConfig(): GeneralXcmConfig {
        return remoteConfig.getString(REMOTE_CONFIG_NAME)
            .flatMap(::parseXcmConfig)
            .getOrThrow()
    }

    override fun xcmConfigFlow(): Flow<GeneralXcmConfig> {
        return flowOf { awaitXcmConfig() }
    }

    private fun parseXcmConfig(raw: String): Result<GeneralXcmConfig> {
        return runCatching {
            val remote = gson.fromJson<GeneralXcmConfigRemote>(raw)
            remote.toDomain()
        }
    }

    private fun GeneralXcmConfigRemote.toDomain(): GeneralXcmConfig {
        return GeneralXcmConfig(
            chains = chains.toDomain(),
            assets = assets.toDomain()
        )
    }

    private fun ChainXcmConfigRemote.toDomain(): ChainXcmConfig {
        return ChainXcmConfig(
            parachainIds = parachainIds
        )
    }

    private fun AssetsXcmConfigRemote.toDomain(): AssetsXcmConfig {
        return AssetsXcmConfig(
            reservesById = assetsLocation.orEmpty()
                .mapValues { (reserveId, reserve) -> reserve.toDomain(reserveId) },
            assetToReserveIdOverrides = reserveIdOverrides.orEmpty()
                .flattenKeys(::FullChainAssetId)
        )
    }

    private fun ChainAssetReserveConfigRemote.toDomain(reserveId: String): ChainAssetReserveConfig {
        return ChainAssetReserveConfig(
            reserveId = reserveId,
            reserveAssetId = FullChainAssetId(chainId, assetId),
            tokenLocation = multiLocation.toAbsoluteLocation(),
        )
    }
}
