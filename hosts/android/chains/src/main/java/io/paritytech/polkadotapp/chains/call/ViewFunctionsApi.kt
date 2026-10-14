package io.paritytech.polkadotapp.chains.call

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.Scale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.decode
import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.novasama.substrate_sdk_android.runtime.definitions.types.composite.Struct
import io.novasama.substrate_sdk_android.runtime.definitions.types.useScaleWriter
import io.novasama.substrate_sdk_android.runtime.metadata.decodeOutput
import io.novasama.substrate_sdk_android.runtime.metadata.module
import io.novasama.substrate_sdk_android.runtime.metadata.module.ViewFunction
import io.novasama.substrate_sdk_android.runtime.metadata.viewFunction
import io.paritytech.polkadotapp.chains.network.binding.ScaleResult
import io.paritytech.polkadotapp.chains.network.binding.mapError
import io.paritytech.polkadotapp.chains.network.binding.toResult
import io.paritytech.polkadotapp.chains.util.EncodedArguments
import io.paritytech.polkadotapp.common.data.substrate.cast
import io.paritytech.polkadotapp.common.utils.flatMap

/**
 * Calls runtime view functions, which are declared per-pallet in metadata v16+.
 *
 * Obtain an instance via [MultiChainViewFunctionsApi.forChain] and call it through the [call] extension.
 */
interface ViewFunctionsApi {
    suspend fun callDynamic(
        pallet: String,
        name: String,
        arguments: Map<String, Any?>,
    ): Result<Any?>
}

suspend inline fun <reified T> ViewFunctionsApi.call(
    pallet: String,
    name: String,
    arguments: EncodedArguments,
): Result<T> {
    return callDynamic(pallet, name, arguments.encoded).mapCatching(Scale::decode)
}

internal class RealViewFunctionsApi(
    private val runtimeCallsApi: RuntimeCallsApi,
) : ViewFunctionsApi {
    companion object {
        private const val VIEW_FUNCTION_API = "RuntimeViewFunction"
        private const val EXECUTE_VIEW_FUNCTION = "execute_view_function"

        private const val QUERY_ID_PARAM = "query_id"
        private const val INPUT_PARAM = "input"

        private const val QUERY_ID_PREFIX_FIELD = "prefix"
        private const val QUERY_ID_SUFFIX_FIELD = "suffix"

        private const val QUERY_ID_HALF_SIZE = 16
    }

    private val runtime: RuntimeSnapshot
        get() = runtimeCallsApi.runtime

    override suspend fun callDynamic(
        pallet: String,
        name: String,
        arguments: Map<String, Any?>,
    ): Result<Any?> {
        return runCatching {
            val viewFunction = runtime.metadata.module(pallet).viewFunction(name)

            val dispatched = runtimeCallsApi.call(
                section = VIEW_FUNCTION_API,
                method = EXECUTE_VIEW_FUNCTION,
                arguments = mapOf(
                    QUERY_ID_PARAM to viewFunction.queryId(),
                    INPUT_PARAM to viewFunction.encodeInput(runtime, arguments)
                ),
                returnBinding = { dispatchResult ->
                    ScaleResult.bind(
                        dynamicInstance = dispatchResult,
                        bindOk = { it.cast<ByteArray>() },
                        bindError = { it }
                    )
                }
            )

            viewFunction to dispatched
        }.flatMap { (viewFunction, dispatched) ->
            // Only the Ok bytes of the dispatch carry the view function's own declared output
            dispatched
                .mapError { "View function $pallet.$name failed to dispatch: $it" }
                .toResult()
                .mapCatching { okBytes -> viewFunction.decodeOutput(runtime, okBytes.toHexString(withPrefix = true)) }
        }
    }

    /**
     * `ViewFunctionId` is declared as `{ prefix: [u8; 16], suffix: [u8; 16] }` — the two halves of the
     * globally-unique 32-byte [ViewFunction.id].
     */
    private fun ViewFunction.queryId(): Struct.Instance {
        return Struct.Instance(
            mapOf(
                QUERY_ID_PREFIX_FIELD to id.copyOfRange(0, QUERY_ID_HALF_SIZE),
                QUERY_ID_SUFFIX_FIELD to id.copyOfRange(QUERY_ID_HALF_SIZE, id.size)
            )
        )
    }

    /**
     * The `input` argument: the view function's own arguments, SCALE-encoded and concatenated in declaration
     * order. The surrounding `Vec<u8>` envelope is applied by the runtime api's declared type.
     */
    private fun ViewFunction.encodeInput(runtime: RuntimeSnapshot, arguments: Map<String, Any?>): ByteArray {
        return useScaleWriter {
            inputs.forEach { param ->
                val type = param.type ?: error("Cannot resolve type for input ${param.name} of $name view function")

                type.encodeUnsafe(this, runtime, arguments.getValue(param.name))
            }
        }
    }
}
