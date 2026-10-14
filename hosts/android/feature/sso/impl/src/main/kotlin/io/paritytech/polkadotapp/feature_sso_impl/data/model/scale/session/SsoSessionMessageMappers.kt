package io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.session

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.WithLength32
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.encodeToByteArray
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.types.BSResult
import io.paritytech.polkadotapp.bandersnatch_crypto.ContextualAlias
import io.paritytech.polkadotapp.chains.util.Sr25519SecretKey
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocatableResource
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocatedResource
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocationOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.CreateProofError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.GetAliasError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ListRingVrfKeysError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.OnExistingAllowancePolicy
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RegisterRingVrfKeyError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingVrfProof
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingVrfSignError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.SlotAccountKey
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.scale.toDomain
import io.paritytech.polkadotapp.feature_products_api.model.scale.toScale
import io.paritytech.polkadotapp.feature_products_api.model.signing.RawPayloadContent
import io.paritytech.polkadotapp.feature_products_api.model.signing.SignedTransaction
import io.paritytech.polkadotapp.feature_products_api.model.signing.SignerPayloadJson
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRawLegacyPayload
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRawPayload
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRequestBody
import io.paritytech.polkadotapp.feature_products_api.model.signing.createTransaction.TxPayload
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionId
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionRequest
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionRequestId
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionResponse
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.EncodedMessage
import io.paritytech.polkadotapp.feature_transactions.api.data.extensions.EncodedTransactionExtensionValue

// ==================== Encoding (Domain -> Scale -> ByteArray) ====================

fun SsoSessionRequest.toSessionMessage(): SsoSessionMessage {
    val statementContent = content.toMessageContent()
    val message = SsoSessionMessageV1(statementContent)
    val versioned = VersionedSsoSessionMessage.V1(message)
    return SsoSessionMessage(requestId, versioned)
}

fun SsoSessionRequest.toEncodedMessage(): EncodedMessage {
    val sessionMessage = toSessionMessage()
    return BinaryScale.encodeToByteArray(sessionMessage)
}

fun SsoSessionResponse.toSessionMessage(): SsoSessionMessage {
    val messageContent = content.toMessageContent(respondingTo)
    val message = SsoSessionMessageV1(messageContent)
    val versioned = VersionedSsoSessionMessage.V1(message)
    return SsoSessionMessage(ownRequestId, versioned)
}

fun SsoSessionResponse.toEncodedMessage(): EncodedMessage {
    val sessionMessage = toSessionMessage()
    return BinaryScale.encodeToByteArray(sessionMessage)
}

// ==================== Decoding (ByteArray -> Scale -> Domain) ====================

fun EncodedMessage.decodeSsoSessionMessage(): Result<SsoSessionMessage> {
    return runCatching { BinaryScale.decodeFromByteArray<SsoSessionMessage>(this) }
}

fun EncodedMessage.decodeAlwaysDecodableSsoMessagePart(): Result<AlwaysDecodableSsoMessagePart> {
    return runCatching { BinaryScale.decodeFromByteArray<AlwaysDecodableSsoMessagePart>(this) }
}

fun EncodedMessage.toSsoSessionRequest(sessionId: SsoSessionId, tld: DotNsTld): Result<SsoSessionRequest> {
    return decodeSsoSessionMessage().mapCatching { message ->
        message.toSsoSessionRequest(sessionId, tld)
    }
}

// ==================== Scale -> Domain mappers ====================

private fun SsoSessionMessage.toSsoSessionRequest(sessionId: SsoSessionId, tld: DotNsTld): SsoSessionRequest {
    return when (versioned) {
        is VersionedSsoSessionMessage.V1 -> versioned.message.toSsoSessionRequest(id, sessionId, tld)
    }
}

private fun SsoSessionMessageV1.toSsoSessionRequest(
    messageId: String,
    sessionId: SsoSessionId,
    tld: DotNsTld
): SsoSessionRequest {
    val requestContent = content.toRequestContent(tld)
    return SsoSessionRequest(
        sessionId = sessionId,
        requestId = messageId,
        content = requestContent
    )
}

private fun SsoMessageContent.toRequestContent(tld: DotNsTld): SsoSessionRequest.Content {
    return when (this) {
        SsoMessageContent.Disconnected -> SsoSessionRequest.Content.Disconnected
        is SsoMessageContent.SigningRequest -> SsoSessionRequest.Content.SigningRequest(request.toDomain())
        is SsoMessageContent.SigningResponse -> error("SigningResponse is a response-only message type")
        is SsoMessageContent.RingVrfAliasRequest -> SsoSessionRequest.Content.AliasRequest(
            callingProduct = ProductId.fromStoredValue(callingProductId),
            keyHandle = keyHandle.toDomain().getOrThrow(),
            context = context.toDomain().getOrThrow(),
            ring = ring.toDomain(),
        )
        is SsoMessageContent.RingVrfAliasResponse -> error("RingVrfAliasResponse is a response-only message type")
        is SsoMessageContent.RingVrfProofRequest -> SsoSessionRequest.Content.CreateProofRequest(
            callingProduct = ProductId.fromStoredValue(callingProductId),
            keyHandle = keyHandle.toDomain().getOrThrow(),
            context = context.toDomain().getOrThrow(),
            ring = ring.toDomain(),
            message = message.value,
        )
        is SsoMessageContent.RingVrfProofResponse -> error("RingVrfProofResponse is a response-only message type")
        is SsoMessageContent.SignVrfRequest -> SsoSessionRequest.Content.SignVrfRequest(
            callingProduct = ProductId.fromStoredValue(callingProductId),
            account = account.toDomain().getOrThrow(),
            transcriptLabel = transcriptLabel.value,
            items = items.map { it.toDomain() },
        )
        is SsoMessageContent.SignVrfResponse -> error("SignVrfResponse is a response-only message type")
        is SsoMessageContent.ResourceAllocationRequest -> SsoSessionRequest.Content.ResourceAllocationRequest(
            callingProduct = ProductId.fromStoredValue(request.callingProductId),
            resources = request.resources.map { it.toDomain() },
            onExisting = request.onExisting.toDomain(),
        )
        is SsoMessageContent.ResourceAllocationResponse -> error("ResourceAllocationResponse is a response-only message type")
        is SsoMessageContent.CreateTransactionRequest -> SsoSessionRequest.Content.CreateTransactionRequest(
            SigningRequestBody.CreateTransaction(request.toDomain())
        )
        is SsoMessageContent.CreateTransactionResponse -> error("CreateTransactionResponse is a response-only message type")
        is SsoMessageContent.CreateTransactionLegacyRequest -> SsoSessionRequest.Content.CreateTransactionLegacyRequest(
            SigningRequestBody.CreateTransactionLegacy(request.toDomain())
        )
        is SsoMessageContent.SignRawLegacyRequest -> SsoSessionRequest.Content.SignRawLegacyRequest(
            SigningRequestBody.RawLegacy(request.toDomain())
        )
        is SsoMessageContent.SignRawLegacyResponse -> error("SignRawLegacyResponse is a response-only message type")
        is SsoMessageContent.ProductSubtreeRequest -> SsoSessionRequest.Content.ProductSubtreeRequest(
            productId = ProductId.fromString(productId, tld).getOrThrow(),
        )
        is SsoMessageContent.ProductSubtreeResponse -> error("ProductSubtreeResponse is a response-only message type")
        is SsoMessageContent.RegisterRingVrfKeyRequest -> SsoSessionRequest.Content.RegisterRingVrfKeyRequest(
            callingProduct = ProductId.fromStoredValue(callingProductId),
            index = index.toDomain().getOrThrow(),
            ring = ring.toDomain(),
        )
        is SsoMessageContent.RegisterRingVrfKeyResponse -> error("RegisterRingVrfKeyResponse is a response-only message type")
        is SsoMessageContent.ListRingVrfKeysRequest -> SsoSessionRequest.Content.ListRingVrfKeysRequest(
            callingProduct = ProductId.fromStoredValue(callingProductId),
            owner = ProductId.fromStoredValue(owner),
            disclosure = disclosure.toDomain(),
        )
        is SsoMessageContent.ListRingVrfKeysResponse -> error("ListRingVrfKeysResponse is a response-only message type")
        is SsoMessageContent.RingVrfSignRequest -> SsoSessionRequest.Content.RingVrfSignRequest(
            callingProduct = ProductId.fromStoredValue(callingProductId),
            keyHandle = keyHandle.toDomain().getOrThrow(),
            message = message.value,
        )
        is SsoMessageContent.RingVrfSignResponse -> error("RingVrfSignResponse is a response-only message type")
    }
}

// ==================== Domain -> Scale mappers ====================

private fun SsoSessionRequest.Content.toMessageContent(): SsoMessageContent {
    return when (this) {
        SsoSessionRequest.Content.Disconnected -> SsoMessageContent.Disconnected
        is SsoSessionRequest.Content.SigningRequest -> SsoMessageContent.SigningRequest(request.toScale())
        is SsoSessionRequest.Content.CreateTransactionRequest -> SsoMessageContent.CreateTransactionRequest(request.payload.toCreateTransactionRequestScale())
        is SsoSessionRequest.Content.CreateTransactionLegacyRequest ->
            SsoMessageContent.CreateTransactionLegacyRequest(request.payload.toCreateTransactionLegacyRequestScale())
        is SsoSessionRequest.Content.SignRawLegacyRequest ->
            SsoMessageContent.SignRawLegacyRequest(request.payload.toScale())
        is SsoSessionRequest.Content.AliasRequest -> SsoMessageContent.RingVrfAliasRequest(
            callingProductId = callingProduct.value,
            keyHandle = keyHandle.toScale(),
            context = context.toScale(),
            ring = ring.toScale(),
        )
        is SsoSessionRequest.Content.CreateProofRequest -> SsoMessageContent.RingVrfProofRequest(
            callingProductId = callingProduct.value,
            keyHandle = keyHandle.toScale(),
            context = context.toScale(),
            ring = ring.toScale(),
            message = message.toDataByteArray(),
        )
        is SsoSessionRequest.Content.SignVrfRequest -> SsoMessageContent.SignVrfRequest(
            callingProductId = callingProduct.value,
            account = account.toScale(),
            transcriptLabel = transcriptLabel.toDataByteArray(),
            items = items.map { it.toScale() },
        )
        is SsoSessionRequest.Content.ResourceAllocationRequest -> SsoMessageContent.ResourceAllocationRequest(
            request = SsoResourceAllocationRequestScale(
                callingProductId = callingProduct.value,
                resources = resources.map { it.toScale() },
                onExisting = onExisting.toScale(),
            )
        )
        is SsoSessionRequest.Content.ProductSubtreeRequest -> SsoMessageContent.ProductSubtreeRequest(productId.value)
        is SsoSessionRequest.Content.RegisterRingVrfKeyRequest -> SsoMessageContent.RegisterRingVrfKeyRequest(
            callingProductId = callingProduct.value,
            index = index.toScale(),
            ring = ring.toScale(),
        )
        is SsoSessionRequest.Content.ListRingVrfKeysRequest -> SsoMessageContent.ListRingVrfKeysRequest(
            callingProductId = callingProduct.value,
            owner = owner.value,
            disclosure = disclosure.toScale(),
        )
        is SsoSessionRequest.Content.RingVrfSignRequest -> SsoMessageContent.RingVrfSignRequest(
            callingProductId = callingProduct.value,
            keyHandle = keyHandle.toScale(),
            message = message.toDataByteArray(),
        )
    }
}

private fun SsoSessionResponse.Content.toMessageContent(respondingTo: SsoSessionRequestId): SsoMessageContent {
    return when (this) {
        is SsoSessionResponse.Content.SignedPayload -> SsoMessageContent.SigningResponse(respondingTo, BSResult.Ok(signed.toScale()))
        is SsoSessionResponse.Content.FailedToSignTransaction -> SsoMessageContent.SigningResponse(respondingTo, BSResult.Err(error))
        is SsoSessionResponse.Content.SignedGeneralTransaction -> SsoMessageContent.CreateTransactionResponse(respondingTo, BSResult.Ok(signedTx))
        is SsoSessionResponse.Content.FailedToCreateTransaction -> SsoMessageContent.CreateTransactionResponse(respondingTo, BSResult.Err(error))
        is SsoSessionResponse.Content.SignedRawLegacy -> SsoMessageContent.SignRawLegacyResponse(respondingTo, BSResult.Ok(signature))
        is SsoSessionResponse.Content.FailedToSignRawLegacy -> SsoMessageContent.SignRawLegacyResponse(respondingTo, BSResult.Err(error))
        is SsoSessionResponse.Content.AliasResult -> SsoMessageContent.RingVrfAliasResponse(respondingTo, BSResult.Ok(alias.toScale()))
        is SsoSessionResponse.Content.FailedToGetAlias -> SsoMessageContent.RingVrfAliasResponse(respondingTo, BSResult.Err(error.toSsoScale()))
        is SsoSessionResponse.Content.ProofResult -> SsoMessageContent.RingVrfProofResponse(respondingTo, BSResult.Ok(proof.toScale()))
        is SsoSessionResponse.Content.FailedToCreateProof -> SsoMessageContent.RingVrfProofResponse(respondingTo, BSResult.Err(error.toSsoScale()))
        is SsoSessionResponse.Content.RegisterRingVrfKeyResult ->
            SsoMessageContent.RegisterRingVrfKeyResponse(respondingTo, BSResult.Ok(WithLength32(publicKey.value)))
        is SsoSessionResponse.Content.FailedToRegisterRingVrfKey ->
            SsoMessageContent.RegisterRingVrfKeyResponse(respondingTo, BSResult.Err(error.toSsoScale()))
        is SsoSessionResponse.Content.ListRingVrfKeysResult ->
            SsoMessageContent.ListRingVrfKeysResponse(respondingTo, BSResult.Ok(entries.map { it.toScale() }))
        is SsoSessionResponse.Content.FailedToListRingVrfKeys ->
            SsoMessageContent.ListRingVrfKeysResponse(respondingTo, BSResult.Err(error.toSsoScale()))
        is SsoSessionResponse.Content.RingVrfSignResult ->
            SsoMessageContent.RingVrfSignResponse(respondingTo, BSResult.Ok(signature))
        is SsoSessionResponse.Content.FailedToRingVrfSign ->
            SsoMessageContent.RingVrfSignResponse(respondingTo, BSResult.Err(error.toSsoScale()))
        is SsoSessionResponse.Content.SignVrfResult -> SsoMessageContent.SignVrfResponse(respondingTo, BSResult.Ok(signature.toScale()))
        is SsoSessionResponse.Content.FailedToSignVrf -> SsoMessageContent.SignVrfResponse(respondingTo, BSResult.Err(error.toSsoScale()))
        is SsoSessionResponse.Content.ResourceAllocationResult -> SsoMessageContent.ResourceAllocationResponse(
            respondingTo = respondingTo,
            payload = BSResult.Ok(outcomes.map { it.toScale() }),
        )
        is SsoSessionResponse.Content.FailedToAllocateResources -> SsoMessageContent.ResourceAllocationResponse(
            respondingTo = respondingTo,
            payload = BSResult.Err(error),
        )
        is SsoSessionResponse.Content.ProductSubtreeResult -> SsoMessageContent.ProductSubtreeResponse(
            respondingTo = respondingTo,
            productPublicKey = BSResult.Ok(WithLength32(productPublicKey.value)))

        is SsoSessionResponse.Content.FailedToGetProductSubtree -> SsoMessageContent.ProductSubtreeResponse(
            respondingTo = respondingTo,
            productPublicKey = BSResult.Err(error),
        )
    }
}

private fun ApAllocatableResource.toScale(): SsoApAllocatableResourceScale = when (this) {
    ApAllocatableResource.StatementStoreAllowance -> SsoApAllocatableResourceScale.StatementStoreAllowance
    ApAllocatableResource.BulletInAllowance -> SsoApAllocatableResourceScale.BulletInAllowance
    is ApAllocatableResource.SmartContractAllowance -> SsoApAllocatableResourceScale.SmartContractAllowance(dest.toScale())
    ApAllocatableResource.AutoSigning -> SsoApAllocatableResourceScale.AutoSigning
}

private fun SsoApAllocatableResourceScale.toDomain(): ApAllocatableResource = when (this) {
    SsoApAllocatableResourceScale.StatementStoreAllowance -> ApAllocatableResource.StatementStoreAllowance
    SsoApAllocatableResourceScale.BulletInAllowance -> ApAllocatableResource.BulletInAllowance
    is SsoApAllocatableResourceScale.SmartContractAllowance -> ApAllocatableResource.SmartContractAllowance(dest.toDomain().getOrThrow())
    SsoApAllocatableResourceScale.AutoSigning -> ApAllocatableResource.AutoSigning
}

private fun OnExistingAllowancePolicy.toScale(): SsoOnExistingAllowancePolicyScale = when (this) {
    OnExistingAllowancePolicy.IGNORE -> SsoOnExistingAllowancePolicyScale.IGNORE
    OnExistingAllowancePolicy.INCREASE -> SsoOnExistingAllowancePolicyScale.INCREASE
}

private fun SsoOnExistingAllowancePolicyScale.toDomain(): OnExistingAllowancePolicy = when (this) {
    SsoOnExistingAllowancePolicyScale.IGNORE -> OnExistingAllowancePolicy.IGNORE
    SsoOnExistingAllowancePolicyScale.INCREASE -> OnExistingAllowancePolicy.INCREASE
}

private fun ApAllocationOutcome.toScale(): SsoApAllocationOutcomeScale = when (this) {
    is ApAllocationOutcome.Allocated -> SsoApAllocationOutcomeScale.Allocated(resource.toScale())
    ApAllocationOutcome.Rejected -> SsoApAllocationOutcomeScale.Rejected
    ApAllocationOutcome.NotAvailable -> SsoApAllocationOutcomeScale.NotAvailable
}

private fun ApAllocatedResource.toScale(): SsoApAllocatedResourceScale = when (this) {
    is ApAllocatedResource.StatementStoreAllowance ->
        SsoApAllocatedResourceScale.StatementStoreAllowance(slotAccountKey.bytes.value)
    is ApAllocatedResource.BulletInAllowance ->
        SsoApAllocatedResourceScale.BulletInAllowance(slotAccountKey.bytes.value)
    ApAllocatedResource.SmartContractAllowance -> SsoApAllocatedResourceScale.SmartContractAllowance
    is ApAllocatedResource.AutoSigning -> SsoApAllocatedResourceScale.AutoSigning(productRootSecretKey.bytes.value)
}

@Suppress("unused")
private fun SsoApAllocationOutcomeScale.toDomain(): ApAllocationOutcome = when (this) {
    is SsoApAllocationOutcomeScale.Allocated -> ApAllocationOutcome.Allocated(resource.toDomain())
    SsoApAllocationOutcomeScale.Rejected -> ApAllocationOutcome.Rejected
    SsoApAllocationOutcomeScale.NotAvailable -> ApAllocationOutcome.NotAvailable
}

@Suppress("unused")
private fun SsoApAllocatedResourceScale.toDomain(): ApAllocatedResource = when (this) {
    is SsoApAllocatedResourceScale.StatementStoreAllowance ->
        ApAllocatedResource.StatementStoreAllowance(SlotAccountKey.fromBytes(DataByteArray(slotAccountKey)).getOrThrow())
    is SsoApAllocatedResourceScale.BulletInAllowance ->
        ApAllocatedResource.BulletInAllowance(SlotAccountKey.fromBytes(DataByteArray(slotAccountKey)).getOrThrow())
    SsoApAllocatedResourceScale.SmartContractAllowance -> ApAllocatedResource.SmartContractAllowance
    is SsoApAllocatedResourceScale.AutoSigning ->
        ApAllocatedResource.AutoSigning(Sr25519SecretKey.fromBytes(DataByteArray(productRootSecretKey)).getOrThrow())
}

// ==================== SigningRequest mappers ====================

private fun SsoSigningRequestScale.toDomain(): SigningRequestBody.ResultHasSignature {
    return when (this) {
        is SsoSigningRequestScale.Transaction -> SigningRequestBody.Transaction(payload.toDomain())
        is SsoSigningRequestScale.RawPayload -> SigningRequestBody.Raw(payload.toDomain())
    }
}

private fun SigningRequestBody.ResultHasSignature.toScale(): SsoSigningRequestScale {
    return when (this) {
        is SigningRequestBody.Transaction -> SsoSigningRequestScale.Transaction(payload.toScale())
        is SigningRequestBody.Raw -> SsoSigningRequestScale.RawPayload(payload.toScale())
    }
}

// ==================== SigningRawPayload mappers ====================

private fun SsoSigningRawPayloadScale.toDomain(): SigningRawPayload {
    return SigningRawPayload(
        account = account.toDomain().getOrThrow(),
        type = type.toDomain()
    )
}

private fun SigningRawPayload.toScale(): SsoSigningRawPayloadScale {
    return SsoSigningRawPayloadScale(
        account = account.toScale(),
        type = type.toScale()
    )
}

private fun SsoPayloadTypeScale.toDomain(): RawPayloadContent {
    return when (this) {
        is SsoPayloadTypeScale.Bytes -> RawPayloadContent.Bytes(data)
        is SsoPayloadTypeScale.Payload -> RawPayloadContent.Payload(data)
    }
}

private fun RawPayloadContent.toScale(): SsoPayloadTypeScale {
    return when (this) {
        is RawPayloadContent.Bytes -> SsoPayloadTypeScale.Bytes(data)
        is RawPayloadContent.Payload -> SsoPayloadTypeScale.Payload(data)
    }
}

// ==================== SignerPayloadJson mappers ====================

private fun SsoSignerPayloadJsonScale.toDomain(): SignerPayloadJson<ProductAccountId> {
    return SignerPayloadJson(
        account = account.toDomain().getOrThrow(),
        blockHash = blockHash,
        blockNumber = blockNumber,
        era = era,
        genesisHash = genesisHash,
        method = method,
        nonce = nonce,
        specVersion = specVersion,
        tip = tip,
        transactionVersion = transactionVersion,
        signedExtensions = signedExtensions,
        version = version,
        assetId = assetId,
        metadataHash = metadataHash,
        mode = mode,
        withSignedTransaction = withSignedTransaction,
    )
}

private fun SignerPayloadJson<ProductAccountId>.toScale(): SsoSignerPayloadJsonScale {
    return SsoSignerPayloadJsonScale(
        account = account.toScale(),
        blockHash = blockHash,
        blockNumber = blockNumber,
        era = era,
        genesisHash = genesisHash,
        method = method,
        nonce = nonce,
        specVersion = specVersion,
        tip = tip,
        transactionVersion = transactionVersion,
        signedExtensions = signedExtensions,
        version = version,
        assetId = assetId,
        metadataHash = metadataHash,
        mode = mode,
        withSignedTransaction = withSignedTransaction,
    )
}

// ==================== SignedTransaction mappers ====================

private fun SignedTransaction.WithDedicatedSignature.toScale(): SsoSignedPayloadJsonScale {
    return when (this) {
        is SignedTransaction.PayloadJson -> SsoSignedPayloadJsonScale(
            signature = signature.value,
            signedTx = signedTx.value,
        )
        is SignedTransaction.Raw -> SsoSignedPayloadJsonScale(
            signature = signature.value,
            signedTx = null,
        )
    }
}

// ==================== ContextualAlias mappers ====================

private fun ContextualAlias.toScale(): SsoContextualAliasScale {
    return SsoContextualAliasScale(
        context = context.value,
        alias = alias.value,
    )
}

// ==================== RingVrf proof / error mappers ====================

private fun RingVrfProof.toScale(): SsoRingVrfProofScale {
    return SsoRingVrfProofScale(
        proof = proof,
        contextualAlias = contextualAlias.toScale(),
        ringIndex = ringIndex.value.toInt(),
        ringRevision = ringRevision.value,
    )
}

private fun GetAliasError.toSsoScale(): SsoRingVrfErrorScale = when (this) {
    GetAliasError.RingNotFound -> SsoRingVrfErrorScale.RingNotFound
    GetAliasError.NotMember -> SsoRingVrfErrorScale.NotMember
    GetAliasError.KeyNotRegistered -> SsoRingVrfErrorScale.KeyNotRegistered
    GetAliasError.KeyNotInRing -> SsoRingVrfErrorScale.KeyNotInRing
    GetAliasError.Rejected -> SsoRingVrfErrorScale.Rejected
    is GetAliasError.Unknown -> SsoRingVrfErrorScale.Unknown(message ?: "Unknown error")
}

// NotConnected has no wire variant: this host has no session gate, so it is reported as Unknown.
private fun RegisterRingVrfKeyError.toSsoScale(): SsoRingVrfErrorScale = when (this) {
    RegisterRingVrfKeyError.RingNotFound -> SsoRingVrfErrorScale.RingNotFound
    RegisterRingVrfKeyError.Rejected -> SsoRingVrfErrorScale.Rejected
    RegisterRingVrfKeyError.NotConnected -> SsoRingVrfErrorScale.Unknown(message ?: "Not connected")
    is RegisterRingVrfKeyError.Unknown -> SsoRingVrfErrorScale.Unknown(message ?: "Unknown error")
}

private fun ListRingVrfKeysError.toSsoScale(): SsoRingVrfErrorScale = when (this) {
    ListRingVrfKeysError.Rejected -> SsoRingVrfErrorScale.Rejected
    ListRingVrfKeysError.NotConnected -> SsoRingVrfErrorScale.Unknown(message ?: "Not connected")
    is ListRingVrfKeysError.Unknown -> SsoRingVrfErrorScale.Unknown(message ?: "Unknown error")
}

private fun RingVrfSignError.toSsoScale(): SsoRingVrfErrorScale = when (this) {
    RingVrfSignError.KeyNotRegistered -> SsoRingVrfErrorScale.KeyNotRegistered
    RingVrfSignError.NotAllowlisted -> SsoRingVrfErrorScale.NotAllowlisted
    RingVrfSignError.Rejected -> SsoRingVrfErrorScale.Rejected
    RingVrfSignError.NotConnected -> SsoRingVrfErrorScale.Unknown(message ?: "Not connected")
    is RingVrfSignError.Unknown -> SsoRingVrfErrorScale.Unknown(message ?: "Unknown error")
}

private fun CreateProofError.toSsoScale(): SsoRingVrfErrorScale = when (this) {
    CreateProofError.RingNotFound -> SsoRingVrfErrorScale.RingNotFound
    CreateProofError.NotMember -> SsoRingVrfErrorScale.NotMember
    CreateProofError.KeyNotRegistered -> SsoRingVrfErrorScale.KeyNotRegistered
    CreateProofError.KeyNotInRing -> SsoRingVrfErrorScale.KeyNotInRing
    CreateProofError.NotAllowlisted -> SsoRingVrfErrorScale.NotAllowlisted
    CreateProofError.Rejected -> SsoRingVrfErrorScale.Rejected
    is CreateProofError.Unknown -> SsoRingVrfErrorScale.Unknown(message ?: "Unknown error")
}

// ==================== CreateTransaction mappers ====================

// Generic over the signer so the product and legacy flows share one payload mapping.
private fun <ScaleSigner, DomainSigner> SsoTxPayloadScale<ScaleSigner>.toDomain(
    signerToDomain: (ScaleSigner) -> DomainSigner,
): TxPayload<DomainSigner> {
    return TxPayload(
        signer = signerToDomain(signer),
        genesisHash = genesisHash.toDataByteArray(),
        callData = callData.toDataByteArray(),
        extensions = extensions.map { it.toDomain() },
        txExtVersion = txExtVersion,
    )
}

private fun <DomainSigner, ScaleSigner> TxPayload<DomainSigner>.toScale(
    signerToScale: (DomainSigner) -> ScaleSigner,
): SsoTxPayloadScale<ScaleSigner> {
    return SsoTxPayloadScale(
        signer = signerToScale(signer),
        genesisHash = genesisHash.value,
        callData = callData.value,
        extensions = extensions.map { it.toScale() },
        txExtVersion = txExtVersion,
    )
}

private fun SsoCreateTransactionRequestScale.toDomain(): TxPayload<ProductAccountId> {
    return when (val versioned = payload) {
        is SsoVersionedTxPayloadScale.V1 -> versioned.payload.toDomain { it.toDomain().getOrThrow() }
    }
}

private fun TxPayload<ProductAccountId>.toCreateTransactionRequestScale(): SsoCreateTransactionRequestScale {
    return SsoCreateTransactionRequestScale(payload = SsoVersionedTxPayloadScale.V1(toScale { it.toScale() }))
}

private fun SsoEncodedTransactionExtensionValueScale.toDomain(): EncodedTransactionExtensionValue {
    return EncodedTransactionExtensionValue(id = id, implicit = implicit.toDataByteArray(), explicit = explicit.toDataByteArray())
}

private fun EncodedTransactionExtensionValue.toScale(): SsoEncodedTransactionExtensionValueScale {
    return SsoEncodedTransactionExtensionValueScale(id = id, implicit = implicit.value, explicit = explicit.value)
}

// ==================== CreateTransactionLegacy / SignRawLegacy mappers ====================

private fun SsoCreateTransactionLegacyRequestScale.toDomain(): TxPayload<AccountId> {
    return when (val versioned = payload) {
        is SsoVersionedLegacyTxPayloadScale.V1 -> versioned.payload.toDomain { it.value.toDataByteArray() }
    }
}

private fun TxPayload<AccountId>.toCreateTransactionLegacyRequestScale(): SsoCreateTransactionLegacyRequestScale {
    return SsoCreateTransactionLegacyRequestScale(payload = SsoVersionedLegacyTxPayloadScale.V1(toScale { AccountIdScale(it.value) }))
}

private fun SsoSignRawLegacyRequestScale.toDomain(): SigningRawLegacyPayload {
    return SigningRawLegacyPayload(
        account = account.value.toDataByteArray(),
        type = type.toDomain(),
    )
}

private fun SigningRawLegacyPayload.toScale(): SsoSignRawLegacyRequestScale {
    return SsoSignRawLegacyRequestScale(
        account = AccountIdScale(account.value),
        type = type.toScale(),
    )
}
