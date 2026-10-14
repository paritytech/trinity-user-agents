package io.paritytech.polkadotapp.chains.call

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.Scale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.decode
import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.novasama.substrate_sdk_android.runtime.metadata.createRequest
import io.novasama.substrate_sdk_android.runtime.metadata.decodeOutput
import io.novasama.substrate_sdk_android.runtime.metadata.method
import io.novasama.substrate_sdk_android.runtime.metadata.runtimeApi
import io.novasama.substrate_sdk_android.wsrpc.SocketService
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.network.rpc.requests.StateCallAtRequest
import io.paritytech.polkadotapp.chains.network.rpc.stateCall
import io.paritytech.polkadotapp.chains.util.EncodedArguments

interface RuntimeCallsApi {
    val runtime: RuntimeSnapshot

    suspend fun <R> call(
        section: String,
        method: String,
        arguments: Map<String, Any?>,
        at: BlockHash? = null,
        returnBinding: (Any?) -> R,
    ): R
}

suspend inline fun <reified T> RuntimeCallsApi.call(
    section: String,
    method: String,
    arguments: EncodedArguments,
    at: BlockHash? = null,
): T {
    return call(
        section = section,
        method = method,
        arguments = arguments.encoded,
        at = at,
        returnBinding = Scale::decode,
    )
}

internal class RealRuntimeCallsApi(
    override val runtime: RuntimeSnapshot,
    private val socketService: SocketService,
) : RuntimeCallsApi {
    override suspend fun <R> call(
        section: String,
        method: String,
        arguments: Map<String, Any?>,
        at: BlockHash?,
        returnBinding: (Any?) -> R,
    ): R {
        val apiMethod = runtime.metadata.runtimeApi(section).method(method)
        val latestRequest = apiMethod.createRequest(runtime, arguments)
        val request = at?.let { StateCallAtRequest(latestRequest, it) } ?: latestRequest

        val response = socketService.stateCall(request)

        val decoded = response?.let { apiMethod.decodeOutput(runtime, it) }

        return returnBinding(decoded)
    }
}
