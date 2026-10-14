package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.handlerGroups

import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.CallingProductIdProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.ContainerBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.DeviceCapabilityType
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.RemotePermissionRequest

class PermissionHostCalls(
    private val botApi: ProductsBotApi,
    private val callingProductIdProvider: CallingProductIdProvider,
) : HostCallHandlerGroup {
    override fun registerOn(bridge: ContainerBridge) {
        bridge.registerHandler<DevicePermissionParams, Boolean>("devicePermission") { params ->
            botApi.requestDevicePermission(
                callingProductIdProvider.getProductId().getOrThrow(),
                DeviceCapabilityType.valueOf(params.capability)
            )
        }

        bridge.registerHandler<List<RemotePermissionDto>, Boolean>("remotePermission") { dtos ->
            botApi.requestRemotePermissions(
                callingProductIdProvider.getProductId().getOrThrow(),
                dtos.map { it.toRequest() },
            )
        }

        bridge.registerHandler<Unit, Boolean>("allowWebRtcAccess") {
            botApi.allowWebRtcAccess(callingProductIdProvider.getProductId().getOrThrow())
        }
    }
}

private data class DevicePermissionParams(val capability: String)

private data class RemotePermissionDto(
    val tag: String,
    val value: List<String>?,
)

private object RemotePermissionDtoTag {
    const val REMOTE = "Remote"
    const val WEB_RTC = "WebRtc"
    const val CHAIN_SUBMIT = "ChainSubmit"
    const val STATEMENT_SUBMIT = "StatementSubmit"
    const val PREIMAGE_SUBMIT = "PreimageSubmit"
}

private fun RemotePermissionDto.toRequest(): RemotePermissionRequest = when (tag) {
    RemotePermissionDtoTag.REMOTE -> {
        val domains = value.orEmpty()
        require(domains.isNotEmpty()) { "Remote permission requires at least one domain" }
        RemotePermissionRequest.Remote(domains.map { it.lowercase() })
    }
    RemotePermissionDtoTag.WEB_RTC -> RemotePermissionRequest.WebRtc
    RemotePermissionDtoTag.CHAIN_SUBMIT -> RemotePermissionRequest.ChainSubmit
    RemotePermissionDtoTag.STATEMENT_SUBMIT -> RemotePermissionRequest.StatementSubmit
    RemotePermissionDtoTag.PREIMAGE_SUBMIT -> RemotePermissionRequest.PreimageSubmit
    else -> throw IllegalArgumentException("Unknown remote permission tag: $tag")
}
