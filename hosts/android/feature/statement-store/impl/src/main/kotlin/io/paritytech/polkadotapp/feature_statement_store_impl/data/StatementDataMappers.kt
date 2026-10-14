package io.paritytech.polkadotapp.feature_statement_store_impl.data

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.encodeToByteArray
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.feature_statement_store_api.data.Statement
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementData
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import io.paritytech.polkadotapp.feature_statement_store_impl.data.encryption.MultiDeviceEnvelopeEncryption
import io.paritytech.polkadotapp.feature_statement_store_impl.data.models.scale.StructuredStatementData
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.models.StatementTransportEvent

context(communicationEncryption: CommunicationEncryption)
fun StatementTransportEvent.toEncryptedStatementData(): StatementData {
    val structuredData = toStructuredStatementData()
    val scaleEncodedData = BinaryScale.encodeToByteArray<StructuredStatementData.Single>(structuredData)

    return communicationEncryption.encrypt(scaleEncodedData)
}

/**
 * Produces [Statement.Body.data] in the multi-device wire format: the event is first wrapped
 * per-device via [envelopeEncryption], then the whole SCALE envelope is encrypted again with
 * the outer pairwise [CommunicationEncryption] — same outer layer as the single-device path.
 */
context(communicationEncryption: CommunicationEncryption)
suspend fun StatementTransportEvent.toMultiDeviceEncryptedStatementData(
    envelopeEncryption: MultiDeviceEnvelopeEncryption,
    recipients: List<MultiDeviceEnvelopeEncryption.Recipient>,
): StatementData {
    val innerPlaintext = when (this) {
        is StatementTransportEvent.Request -> BinaryScale.encodeToByteArray(StructuredStatementData.Request(messages = messages, requestId = requestId))
        is StatementTransportEvent.Response -> BinaryScale.encodeToByteArray(StructuredStatementData.Response(requestId = requestId, responseCode = responseCode))
    }

    val wrapped = envelopeEncryption.wrap(
        payload = innerPlaintext,
        recipients = recipients,
    )

    val outer: StructuredStatementData = when (this) {
        is StatementTransportEvent.Request -> StructuredStatementData.MultiRequest(
            encryptedRequest = wrapped.encryptedPayload.value,
            devicesInfo = wrapped.devicesInfo,
        )

        is StatementTransportEvent.Response -> StructuredStatementData.MultiResponse(
            encryptedResponse = wrapped.encryptedPayload.value,
            devicesInfo = wrapped.devicesInfo,
        )
    }

    val scaleEncodedData = BinaryScale.encodeToByteArray<StructuredStatementData>(outer)
    return communicationEncryption.encrypt(scaleEncodedData)
}

/**
 * Decodes a [Statement] into a [StatementTransportEvent] regardless of whether the wire
 * format is plain (Request/Response) or multi-device envelope. Envelope variants are
 * unwrapped via [envelopeEncryption] using [senderEncryptionPublicKey].
 */
context(communicationEncryption: CommunicationEncryption)
suspend fun Statement.decryptAndDecodeEvent(
    envelopeEncryption: MultiDeviceEnvelopeEncryption,
    senderEncryptionPublicKey: X25519PublicKey,
): StatementTransportEvent {
    val single = body.data.decryptAndDecodeStructuredSingle(envelopeEncryption, senderEncryptionPublicKey)
    return single.toTransportEvent(body.expiry)
}

/**
 * Decodes one of OUR OWN previously-submitted statements. Multi-device envelopes have no entry
 * for us, so they are unwrapped via a peer device entry ([MultiDeviceEnvelopeEncryption.unwrapOwn]).
 */
context(communicationEncryption: CommunicationEncryption)
suspend fun Statement.decryptAndDecodeOwnEvent(
    envelopeEncryption: MultiDeviceEnvelopeEncryption,
    peerDevices: List<MultiDeviceEnvelopeEncryption.Recipient>,
): StatementTransportEvent {
    val decryptedData = communicationEncryption.decrypt(body.data)
    val statementData = BinaryScale.decodeFromByteArray<StructuredStatementData>(decryptedData)

    val single = when (statementData) {
        is StructuredStatementData.Single -> statementData
        is StructuredStatementData.Multi -> {
            val innerPlaintext = envelopeEncryption.unwrapOwn(
                encryptedPayload = statementData.encryptedPayload,
                devicesInfo = statementData.devicesInfo,
                peerDevices = peerDevices,
            )
            when (statementData) {
                is StructuredStatementData.MultiRequest -> BinaryScale.decodeFromByteArray<StructuredStatementData.Request>(innerPlaintext)
                is StructuredStatementData.MultiResponse -> BinaryScale.decodeFromByteArray<StructuredStatementData.Response>(innerPlaintext)
            }
        }
    }
    return single.toTransportEvent(body.expiry)
}

/**
 * Decrypts the outer pairwise layer of [this] and decodes the resulting payload into a
 * [StructuredStatementData.Single]. Multi-device envelopes are transparently unwrapped via
 * [envelopeEncryption] using [senderEncryptionPublicKey]. Use this when only the inner
 * Request/Response payload is needed and there is no enclosing [Statement] (e.g. wire bytes
 * delivered out-of-band via push notifications).
 */
context(communicationEncryption: CommunicationEncryption)
suspend fun ByteArray.decryptAndDecodeStructuredSingle(
    envelopeEncryption: MultiDeviceEnvelopeEncryption,
    senderEncryptionPublicKey: X25519PublicKey,
): StructuredStatementData.Single {
    val decryptedData = communicationEncryption.decrypt(this)
    val statementData = BinaryScale.decodeFromByteArray<StructuredStatementData>(decryptedData)

    return when (statementData) {
        is StructuredStatementData.Single -> statementData
        is StructuredStatementData.Multi -> {
            val innerPlaintext = envelopeEncryption.unwrap(
                encryptedPayload = statementData.encryptedPayload,
                devicesInfo = statementData.devicesInfo,
                senderEncryptionPublicKey = senderEncryptionPublicKey,
            )

            when (statementData) {
                is StructuredStatementData.MultiRequest -> BinaryScale.decodeFromByteArray<StructuredStatementData.Request>(innerPlaintext)
                is StructuredStatementData.MultiResponse -> BinaryScale.decodeFromByteArray<StructuredStatementData.Response>(innerPlaintext)
            }
        }
    }
}

private fun StatementTransportEvent.toStructuredStatementData(): StructuredStatementData.Single {
    return when (this) {
        is StatementTransportEvent.Request -> StructuredStatementData.Request(
            messages = messages,
            requestId = requestId,
        )

        is StatementTransportEvent.Response -> StructuredStatementData.Response(
            requestId = requestId,
            responseCode = responseCode,
        )
    }
}

private fun StructuredStatementData.Single.toTransportEvent(expiry: ULong): StatementTransportEvent {
    return when (this) {
        is StructuredStatementData.Request -> StatementTransportEvent.Request(
            messages = messages,
            requestId = requestId,
            expiry = expiry,
        )

        is StructuredStatementData.Response -> StatementTransportEvent.Response(
            requestId = requestId,
            responseCode = responseCode,
            expiry = expiry,
        )
    }
}
