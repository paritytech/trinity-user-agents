package io.paritytech.polkadotapp.feature_usernames_impl.domain.usecase

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.FixedLength
import io.paritytech.polkadotapp.chains.util.scaleEncodeBinary
import io.paritytech.polkadotapp.chains.util.signing.MessageSigningContext
import io.paritytech.polkadotapp.common.domain.model.AccountEcdhKey
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.CurrentTimeContext
import io.paritytech.polkadotapp.common.domain.model.EncodedPublicKey
import io.paritytech.polkadotapp.common.domain.model.hexToDataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.combineResults
import io.paritytech.polkadotapp.common.utils.scale.AccountEcdhKeyScaleSerializer
import io.paritytech.polkadotapp.common.utils.scale.encodeOnChain
import io.paritytech.polkadotapp.common.utils.scale.toScale
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.sign.AccountBytesSigner
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.BandersnatchSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.getMemberKey
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_account_api.domain.usecase.SharedSecretDerivationUseCase
import io.paritytech.polkadotapp.feature_dotns_gateway_api.data.repository.DotNsGatewayRepository
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.encoded
import io.paritytech.polkadotapp.feature_usernames_impl.domain.UsernamesChainProvider
import io.paritytech.polkadotapp.feature_usernames_impl.domain.model.ClaimUsernameParams
import kotlinx.serialization.Serializable
import javax.inject.Inject

interface CreateClaimParamsUseCase {
    suspend operator fun invoke(username: Username, attester: String, preferredDigits: String): Result<ClaimUsernameParams>
}

class RealCreateClaimParamsUseCase @Inject constructor(
    private val usernamesChainProvider: UsernamesChainProvider,
    private val bytesSigner: AccountBytesSigner,
    private val accountRepository: AccountRepository,
    private val sharedSecretDerivationUseCase: SharedSecretDerivationUseCase,
    private val bandersnatchSecretsStorage: BandersnatchSecretsStorage,
    private val currentTimeContext: CurrentTimeContext,
    private val dotNsGatewayRepository: DotNsGatewayRepository
) : CreateClaimParamsUseCase {
    companion object {
        private const val MESSAGE_PREFIX = "pop:people-lite:register using"
    }

    override suspend fun invoke(username: Username, attester: String, preferredDigits: String): Result<ClaimUsernameParams> {
        val reservedUsername = username.base
        val signedAt = currentTimeContext.currentTime().epochSeconds

        return combineResults(
            getCandidateSignature(),
            getResourcesSignature(username, attester, reservedUsername),
            getMembershipSignature(),
            getDotNsSignature(username, attester, reservedUsername, signedAt),
            ::ClaimSignatures
        ).mapCatching { signatures ->
            ClaimUsernameParams(
                username = username.getDisplayUsername(),
                preferredDigits = preferredDigits,
                ringVrfKey = getMemberKeyBytes(),
                candidateAddress = getUserAddress(),
                candidateSignature = signatures.candidate,
                consumerSignature = signatures.consumer,
                membershipSignature = signatures.membership,
                identifierKey = getChatPublicKey(),
                dotNsSignature = signatures.dotNs,
                dotNsSignedAt = signedAt,
                dotNsReservedUsername = reservedUsername
            )
        }
    }

    private suspend fun getDotNsSignature(
        username: Username,
        attester: String,
        reservedUsername: String,
        signedAt: Long
    ): Result<ByteArray> {
        val payload = dotNsGatewayRepository.reservationSigningPayload(
            candidate = getAccountId(),
            attester = attester.hexToDataByteArray(),
            usernameBase = username.base,
            chatKey = getChatPublicKey(),
            reservedBaseLabel = reservedUsername,
            signedAt = signedAt
        )

        return bytesSigner.signRawBytesByWallet(payload, usernamesChainProvider.chainId, MessageSigningContext.trustedContent())
            .map { it.signature }
    }

    private suspend fun getAccountId(): AccountId {
        return accountRepository.getWalletAccount()
            .accountIdIn(usernamesChainProvider.chain())
    }

    private suspend fun getPublicKey(): ByteArray {
        return accountRepository.getWalletAccount()
            .accountIdIn(usernamesChainProvider.chain()).value
    }

    private suspend fun getUserAddress(): String {
        return accountRepository.getWalletAccount()
            .addressIn(usernamesChainProvider.chain())
    }

    private suspend fun getMemberKeyBytes(): ByteArray {
        return bandersnatchSecretsStorage.getMemberKey(accountRepository.getWalletAccount().id)
            .value
    }

    // RFC-0004: on-chain records keep the 65-byte slot, with a keypair-type marker in its first byte.
    private suspend fun getChatPublicKey(): EncodedPublicKey {
        val keypair = sharedSecretDerivationUseCase.deriveForDomain(SharedSecretDerivationDomain.CHAT)
        val scaleModel = AccountEcdhKey.X25519(keypair.publicKey).toScale()
        return scaleModel.encodeOnChain().toDataByteArray()
    }

    private suspend fun getMembershipSignature(): Result<ByteArray> {
        return bytesSigner.signWithBandersnatchByWallet(getLitePersonSignatureData(), MessageSigningContext.trustedContent())
    }

    private suspend fun getResourcesSignature(
        username: Username,
        attester: String,
        reservedUsername: String
    ): Result<ByteArray> {
        val publicKey = getPublicKey()
        val chatPublicKey = getChatPublicKey()

        val data = ResourcesSignatureMessage(
            publicKey = publicKey,
            verifier = attester.fromHex(),
            chatPublicKey = chatPublicKey.value,
            username = username.encoded(),
            reservedUsername = reservedUsername.encodeToByteArray()
        ).scaleEncodeBinary()

        return bytesSigner.signRawBytesByWallet(data, usernamesChainProvider.chainId, MessageSigningContext.trustedContent())
            .map { it.signature }
    }

    private suspend fun getCandidateSignature(): Result<ByteArray> {
        val data = getLitePersonSignatureData()
        return bytesSigner.signRawBytesByWallet(data, usernamesChainProvider.chainId, MessageSigningContext.trustedContent())
            .map { it.signature }
    }

    private suspend fun getLitePersonSignatureData() =
        MESSAGE_PREFIX.encodeToByteArray() + getPublicKey() + getMemberKeyBytes()
}

@Serializable
private class ResourcesSignatureMessage(
    @FixedLength(32) val publicKey: ByteArray,
    @FixedLength(32) val verifier: ByteArray,
    @FixedLength(AccountEcdhKeyScaleSerializer.CONTAINER_SIZE_BYTES) val chatPublicKey: ByteArray,
    val username: ByteArray,
    val reservedUsername: ByteArray?
)

private class ClaimSignatures(
    val candidate: ByteArray,
    val consumer: ByteArray,
    val membership: ByteArray,
    val dotNs: ByteArray
)
