package io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.config

import io.paritytech.polkadotapp.common.domain.model.DataByteArray

class DotNsGatewayConfig(
    val popControllerAddress: DataByteArray,
    val popResolverAddress: DataByteArray
)

interface DotNsGatewayConfigProvider {
    suspend fun getConfig(): Result<DotNsGatewayConfig>
}
