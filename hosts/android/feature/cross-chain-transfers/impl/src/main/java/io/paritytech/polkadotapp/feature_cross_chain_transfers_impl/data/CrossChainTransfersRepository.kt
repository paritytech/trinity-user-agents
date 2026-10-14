package io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.data

import com.google.gson.Gson
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.fromJson
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.data.model.DynamicCrossChainTransfersConfigRemote
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.data.model.toDomain
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration.CrossChainTransfersDirectionsConfiguration
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import javax.inject.Inject

internal interface CrossChainTransfersRepository {
    suspend fun getDirectionsConfiguration(): Result<CrossChainTransfersDirectionsConfiguration>
}

internal class RealCrossChainTransfersRepository @Inject constructor(
    private val gson: Gson,
    private val remoteConfig: RemoteConfigService,
) : CrossChainTransfersRepository {
    companion object {
        private const val REMOTE_CONFIG_KEY = "cross_chain_transfers"
    }

    override suspend fun getDirectionsConfiguration(): Result<CrossChainTransfersDirectionsConfiguration> {
        return withContext(Dispatchers.Default) {
            remoteConfig.getSyncedString(REMOTE_CONFIG_KEY)
                .flatMap(::parseDirections)
        }
    }

    private fun parseDirections(raw: String): Result<CrossChainTransfersDirectionsConfiguration> {
        return runCatching {
            gson.fromJson<DynamicCrossChainTransfersConfigRemote>(raw)
                .toDomain()
        }
    }
}
