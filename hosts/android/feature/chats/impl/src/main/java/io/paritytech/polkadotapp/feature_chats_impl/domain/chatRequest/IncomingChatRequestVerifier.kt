package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.domain.model.x25519OrNull
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chain_resources_api.data.repository.ResourcesRepository
import io.paritytech.polkadotapp.feature_chain_resources_api.data.repository.requireConsumerInfo
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequestId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.IdentityProof
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.model.VersionedRequestContent
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.model.toDomain
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.signerAccountId
import javax.inject.Inject

interface IncomingChatRequestVerifier {
    suspend fun verify(request: IncomingChatRequestStatement.Request): Result<Unit>

    suspend fun verifyCancel(cancel: IncomingChatRequestStatement.Cancel): Result<Unit>
}

class RealIncomingChatRequestVerifier @Inject constructor(
    private val identityProofCodec: IdentityProofCodec,
    private val resourcesRepository: ResourcesRepository,
    private val chainRegistry: ChainRegistry,
    private val knownChains: KnownChains,
) : IncomingChatRequestVerifier {
    override suspend fun verify(request: IncomingChatRequestStatement.Request): Result<Unit> {
        return when (val content = request.content) {
            is VersionedRequestContent.V1 -> Result.success(Unit)

            is VersionedRequestContent.V2 -> {
                val proof = content.content.identityProof.toDomain()
                peerChatPubKeyOf(proof)
                    .flatMap { peerKey -> identityProofCodec.verify(proof, request.proof.signerAccountId(), peerKey) }
                    .flatMap { verified -> requireVerified(verified, request.requestId) }
            }
        }
    }

    override suspend fun verifyCancel(cancel: IncomingChatRequestStatement.Cancel): Result<Unit> {
        return peerChatPubKeyOf(cancel.identityProof)
            .flatMap { peerKey -> identityProofCodec.verifyCancel(cancel.identityProof, cancel.requestId, peerKey) }
            .flatMap { verified -> requireVerified(verified, cancel.requestId) }
    }

    private fun requireVerified(verified: Boolean, requestId: ChatRequestId): Result<Unit> {
        return if (verified) Result.success(Unit) else Result.failure(IllegalStateException("IdentityProof verification failed for chat request $requestId"))
    }

    private suspend fun peerChatPubKeyOf(proof: IdentityProof): Result<X25519PublicKey> {
        return runCancellableCatching { chainRegistry.getChain(knownChains.assetHub) }
            .flatMap { chain -> resourcesRepository.requireConsumerInfo(chain, proof.identityAccountId) }
            .flatMap { consumer ->
                val key = consumer.identifierKey.x25519OrNull()
                if (key != null) Result.success(key) else Result.failure(IllegalStateException("Peer account uses an unsupported chat encryption key type"))
            }
    }
}
