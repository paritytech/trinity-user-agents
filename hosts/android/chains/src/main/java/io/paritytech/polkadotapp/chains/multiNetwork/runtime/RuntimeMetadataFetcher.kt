package io.paritytech.polkadotapp.chains.multiNetwork.runtime

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.scale.dataType.list
import io.novasama.substrate_sdk_android.scale.dataType.toHex
import io.novasama.substrate_sdk_android.scale.dataType.uint32
import io.novasama.substrate_sdk_android.wsrpc.SocketService
import io.novasama.substrate_sdk_android.wsrpc.request.runtime.state.StateCallRequest
import io.paritytech.polkadotapp.chains.network.rpc.stateCall
import javax.inject.Inject
import javax.inject.Singleton

private const val LATEST_SUPPORTED_METADATA_VERSION = 16

@Singleton
class RuntimeMetadataFetcher @Inject constructor() {
    suspend fun fetchRawMetadata(socketService: SocketService): RawRuntimeMetadata {
        return socketService.fetchNewestMetadata()
    }

    private suspend fun SocketService.fetchNewestMetadata(): RawRuntimeMetadata {
        val availableVersions = getAvailableMetadataVersions()
        val latestSupported = availableVersions.sorted().lastOrNull { it <= LATEST_SUPPORTED_METADATA_VERSION }
        requireNotNull(latestSupported) {
            "Node reports no metadata version we support: $availableVersions"
        }

        return getMetadataAtVersion(latestSupported)
    }

    private suspend fun SocketService.getAvailableMetadataVersions(): List<Int> {
        val request = StateCallRequest(runtimeRpcName = "Metadata_metadata_versions", "0x")
        return stateCall(request, returnType = list(uint32)).map { it.toInt() }
    }

    private suspend fun SocketService.getMetadataAtVersion(version: Int): ByteArray {
        val versionEncoded = uint32.toHex(version.toUInt())
        val request = StateCallRequest("Metadata_metadata_at_version", versionEncoded)
        val response = stateCall(request)
        requireNotNull(response) {
            "Non existent metadata"
        }

        return response.fromHex()
    }
}
