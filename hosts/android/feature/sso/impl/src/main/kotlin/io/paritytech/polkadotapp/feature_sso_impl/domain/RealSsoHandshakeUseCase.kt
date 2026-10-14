package io.paritytech.polkadotapp.feature_sso_impl.domain

import io.paritytech.polkadotapp.common.domain.model.EncodedPublicKey
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.common.utils.progressStallReport.markRegion
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_account_api.domain.usecase.AccountDerivationUseCase
import io.paritytech.polkadotapp.feature_account_api.domain.usecase.SharedSecretDerivationUseCase
import io.paritytech.polkadotapp.feature_products_api.domain.deriveEntropy.DeriveEntropyUseCase
import io.paritytech.polkadotapp.feature_sso_api.domain.HandshakeResponse
import io.paritytech.polkadotapp.feature_sso_api.domain.SsoHandshakeUseCase
import io.paritytech.polkadotapp.feature_sso_api.domain.model.HandshakeOffer
import io.paritytech.polkadotapp.feature_sso_impl.data.HandshakeProtocolResponse
import io.paritytech.polkadotapp.feature_sso_impl.data.HandshakeSuccessPayload
import io.paritytech.polkadotapp.feature_sso_impl.data.SsoHandshakeProtocol
import io.paritytech.polkadotapp.feature_sso_impl.data.SsoHandshakeRepository
import io.paritytech.polkadotapp.feature_sso_impl.data.encryption.SsoDerivationDomains
import io.paritytech.polkadotapp.feature_sso_impl.domain.model.HandshakeAnswer
import io.paritytech.polkadotapp.feature_statement_store_api.data.Statement
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementData
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import io.paritytech.polkadotapp.feature_statement_store_api.domain.OurDeviceKeypairProvider
import kotlinx.coroutines.withContext
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

class RealSsoHandshakeUseCase @Inject constructor(
    private val communicationEncryption: CommunicationEncryption.Factory,
    private val handshakeRepository: SsoHandshakeRepository,
    private val accountRepository: AccountRepository,
    private val handshakeProtocol: SsoHandshakeProtocol,
    private val accountDerivationUseCase: AccountDerivationUseCase,
    private val sharedSecretDerivationUseCase: SharedSecretDerivationUseCase,
    private val deriveEntropyUseCase: DeriveEntropyUseCase,
    private val ourDeviceKeypairProvider: OurDeviceKeypairProvider,
    private val coroutineDispatchers: CoroutineDispatchers,
) : SsoHandshakeUseCase {
    context(diagnostics: StalenessReportCollector)
    override suspend fun respondToOffer(
        offer: HandshakeOffer,
        response: HandshakeResponse,
    ): Result<Unit> = diagnostics.markRegion(RCommon.string.sso_stall_responding_to_device) {
        withContext(coroutineDispatchers.io) {
            val rootAccount = accountDerivationUseCase.deriveRootAccount()
            val walletAccount = accountRepository.getWalletAccount()

            rootAccount.mapCatching { rootAccountId ->
                val protocolResponse = response.toProtocol(rootAccountId)
                val inner = handshakeProtocol.encodeResponse(protocolResponse)
                deriveAnswerBody(offer, inner)
            }.flatMap { handshakeRepository.submitHandshakeAnswer(it, walletAccount) }
        }
    }

    private suspend fun HandshakeResponse.toProtocol(rootAccountId: EncodedPublicKey): HandshakeProtocolResponse = when (this) {
        HandshakeResponse.AllowanceAllocation -> HandshakeProtocolResponse.AllowanceAllocation
        HandshakeResponse.Success -> HandshakeProtocolResponse.Success(buildSuccessPayload(rootAccountId))
        is HandshakeResponse.Failure -> HandshakeProtocolResponse.Failure(reason)
    }

    private suspend fun buildSuccessPayload(rootAccountId: EncodedPublicKey): HandshakeSuccessPayload {
        val walletAccount = accountRepository.getWalletAccount()

        val identityChatKeypair = sharedSecretDerivationUseCase.deriveForDomain(SharedSecretDerivationDomain.CHAT)
        val identityChatPrivateKey = identityChatKeypair.privateKey.bytes

        val ssoSessionKeypair = sharedSecretDerivationUseCase.deriveForDomain(SsoDerivationDomains.SSO_DERIVATION_DOMAIN)
        val ssoEncrPubKey = ssoSessionKeypair.publicKey

        val rootEntropySource = deriveEntropyUseCase.deriveRootEntropySource().getOrThrow()

        return HandshakeSuccessPayload(
            identityAccountId = walletAccount.defaultAccountId().value,
            rootAccountId = rootAccountId.value,
            identityChatPrivateKey = identityChatPrivateKey,
            ssoEncrPubKey = ssoEncrPubKey,
            deviceEncPubKey = ourDeviceKeypairProvider.publicKey(),
            rootEntropySource = rootEntropySource,
        )
    }

    private suspend fun deriveAnswerBody(
        offer: HandshakeOffer,
        innerPayload: ByteArray,
    ): Statement.Body {
        val tempEncryption = communicationEncryption.createOneTimeUse(offer.device.encryptionPublicKey)
        val encryptedPayload = tempEncryption.encrypt(innerPayload)

        val answer = HandshakeAnswer(encryptedPayload, tempEncryption.localPublicKey)
        val statementData = handshakeProtocol.encodeAnswerStatementData(answer)

        return prepareAnswerBody(offer, statementData)
    }

    private fun prepareAnswerBody(
        offer: HandshakeOffer,
        statementData: StatementData,
    ): Statement.Body {
        val statementStorePublicKey = EncodedPublicKey(offer.device.statementAccountId.value)
        return Statement.Body(
            data = statementData,
            topic1 = handshakeProtocol.statementTopic(statementStorePublicKey, offer.device.encryptionPublicKey),
            channel = handshakeProtocol.statementChannel(statementStorePublicKey, offer.device.encryptionPublicKey),
            expiry = handshakeProtocol.statementExpiry()
        )
    }
}
