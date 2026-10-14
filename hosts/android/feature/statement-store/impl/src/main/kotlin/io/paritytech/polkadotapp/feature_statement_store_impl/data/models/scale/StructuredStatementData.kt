package io.paritytech.polkadotapp.feature_statement_store_impl.data.models.scale

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import kotlinx.serialization.Serializable

@Serializable
sealed class StructuredStatementData {
    @Serializable
    sealed interface Single

    @Serializable
    sealed interface Multi {
        val encryptedPayload: ByteArray
        val devicesInfo: List<RequestDeviceInfo>
    }

    @Serializable
    @EnumIndex(0)
    class Request(
        val requestId: String,
        val messages: List<ByteArray>,
    ) : StructuredStatementData(), Single

    @Serializable
    @EnumIndex(1)
    class Response(
        val requestId: String,
        val responseCode: UByte,
    ) : StructuredStatementData(), Single

    @Serializable
    @EnumIndex(2)
    class MultiRequest(
        val encryptedRequest: ByteArray,
        override val devicesInfo: List<RequestDeviceInfo>,
    ) : StructuredStatementData(), Multi {
        override val encryptedPayload: ByteArray get() = encryptedRequest
    }

    @Serializable
    @EnumIndex(3)
    class MultiResponse(
        val encryptedResponse: ByteArray,
        override val devicesInfo: List<RequestDeviceInfo>,
    ) : StructuredStatementData(), Multi {
        override val encryptedPayload: ByteArray get() = encryptedResponse
    }
}
