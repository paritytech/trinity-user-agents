package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import io.paritytech.polkadotapp.bandersnatch_crypto.ContextualAlias
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.GenesisHash
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.PaymentStatus
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.AllocatableResource
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.AllocationOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ProductProofContext
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RegisteredRingVrfKey
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocation
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingVrfKeyDisclosure
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingVrfProof
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.VrfSignature
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.VrfTranscriptItem
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.signing.SignedTransaction
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRequestBody
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.CallingProductIdProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.GetUserIdResult
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.LegacyAccountResult
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.PaymentBalance
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.ProductAccountResult
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.ProductTheme
import io.paritytech.polkadotapp.feature_products_impl.domain.notifications.NotificationId
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.ProductPaymentRequestId
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.DeviceCapabilityType
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.RemotePermissionRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.PaymentTopUpId
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.PaymentTopUpSource
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpStatus
import io.paritytech.polkadotapp.feature_statement_store_api.data.Statement
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementsPage
import io.paritytech.polkadotapp.feature_statement_store_api.data.TopicFilter
import io.paritytech.polkadotapp.feature_statement_store_api.data.models.StatementStoreMessageProof
import kotlinx.coroutines.flow.Flow
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

abstract class BaseProductsBotApi(
    private val hostApiInteractor: HostApiInteractor,
    private val callingProductIdProvider: CallingProductIdProvider
) : ProductsBotApi {
    override suspend fun accountGet(callingProductId: ProductId, productAccountId: ProductAccountId): Result<ProductAccountResult> {
        return hostApiInteractor.accountGet(callingProductId, productAccountId)
    }

    override suspend fun registerRingVrfKey(
        callingProductId: ProductId,
        index: DerivationIndex32,
        ring: RingLocation,
    ): Result<DataByteArray> {
        return hostApiInteractor.registerRingVrfKey(callingProductId, index, ring)
    }

    override suspend fun listRingVrfKeys(
        callingProductId: ProductId,
        owner: ProductId,
        disclosure: RingVrfKeyDisclosure,
    ): Result<List<RegisteredRingVrfKey>> {
        return hostApiInteractor.listRingVrfKeys(callingProductId, owner, disclosure)
    }

    override suspend fun ringVrfSign(
        callingProductId: ProductId,
        keyHandle: ProductAccountId,
        message: ByteArray,
    ): Result<ByteArray> {
        return hostApiInteractor.ringVrfSign(callingProductId, keyHandle, message)
    }

    override suspend fun accountGetAlias(
        callingProductId: ProductId,
        keyHandle: ProductAccountId,
        context: ProductProofContext,
        ring: RingLocation,
    ): Result<ContextualAlias> {
        return hostApiInteractor.accountGetAlias(callingProductId, keyHandle, context, ring)
    }

    override suspend fun accountCreateProof(
        callingProductId: ProductId,
        keyHandle: ProductAccountId,
        context: ProductProofContext,
        ring: RingLocation,
        message: ByteArray,
    ): Result<RingVrfProof> {
        return hostApiInteractor.accountCreateProof(callingProductId, keyHandle, context, ring, message)
    }

    override suspend fun accountSignVrf(
        callingProductId: ProductId,
        account: ProductAccountId,
        transcriptLabel: ByteArray,
        items: List<VrfTranscriptItem>,
    ): Result<VrfSignature> {
        return hostApiInteractor.signVrf(callingProductId, account, transcriptLabel, items)
    }

    override suspend fun getUserId(callingProductId: ProductId): Result<GetUserIdResult> {
        return hostApiInteractor.getUserId(callingProductId)
    }

    override suspend fun getLegacyAccounts(): Result<List<LegacyAccountResult>> {
        return hostApiInteractor.getLegacyAccounts()
    }

    override suspend fun lookupPreimage(hash: ByteArray): Result<ByteArray> {
        return hostApiInteractor.lookupPreimage(hash)
    }

    override suspend fun submitPreimage(callingProductId: ProductId, data: ByteArray): Result<String> {
        return hostApiInteractor.submitPreimage(callingProductId, data)
    }

    override suspend fun createStatementProof(
        callingProductId: ProductId,
        productAccountId: ProductAccountId,
        statementBody: Statement.Body,
    ): Result<StatementStoreMessageProof> {
        return hostApiInteractor.createStatementProof(callingProductId, productAccountId, statementBody)
    }

    override suspend fun createStatementProofAuthorized(
        callingProductId: ProductId,
        statementBody: Statement.Body,
    ): Result<StatementStoreMessageProof> {
        return hostApiInteractor.createStatementProofAuthorized(callingProductId, statementBody)
    }

    override suspend fun requestResourceAllocation(
        callingProductId: ProductId,
        resources: List<AllocatableResource>,
    ): Result<List<AllocationOutcome>> {
        return hostApiInteractor.requestResourceAllocation(callingProductId, resources)
    }

    override suspend fun requestDevicePermission(callingProductId: ProductId, capability: DeviceCapabilityType): Result<Boolean> {
        return hostApiInteractor.requestDevicePermission(callingProductId, capability)
    }

    override suspend fun requestRemotePermissions(
        callingProductId: ProductId,
        requests: List<RemotePermissionRequest>,
    ): Result<Boolean> {
        return hostApiInteractor.requestRemotePermissions(callingProductId, requests)
    }

    override suspend fun allowWebRtcAccess(callingProductId: ProductId): Result<Boolean> {
        return hostApiInteractor.allowWebRtcAccess(callingProductId)
    }

    @OptIn(ExperimentalTime::class)
    override suspend fun publishNotification(
        callingProductId: ProductId,
        text: String,
        deeplink: String?,
        scheduledAt: Instant?,
    ): Result<NotificationId> {
        return hostApiInteractor.publishNotification(callingProductId, text, deeplink, scheduledAt)
    }

    override suspend fun cancelNotification(callingProductId: ProductId, notificationId: NotificationId): Result<Unit> {
        return hostApiInteractor.cancelNotification(callingProductId, notificationId)
    }

    override suspend fun submitStatement(callingProductId: ProductId, statement: Statement): Result<Unit> {
        return hostApiInteractor.submitStatement(callingProductId, statement)
    }

    override fun subscribeStatements(filter: TopicFilter): Flow<StatementsPage> {
        return hostApiInteractor.subscribeStatements(filter)
    }

    override fun subscribePaymentBalance(callingProductId: ProductId): Flow<PaymentBalance> {
        return hostApiInteractor.subscribePaymentBalance(callingProductId)
    }

    override suspend fun requestPayment(
        callingProductId: ProductId,
        id: ProductPaymentRequestId,
        amount: Balance,
        destination: AccountId,
    ): Result<Unit> {
        return hostApiInteractor.requestPayment(callingProductId, id, amount, destination)
    }

    override suspend fun topUp(
        callingProductId: ProductId,
        id: PaymentTopUpId,
        amount: Balance,
        source: PaymentTopUpSource,
    ): Result<Unit> {
        return hostApiInteractor.topUp(callingProductId, id, amount, source)
    }

    override fun subscribeTopUpStatus(callingProductId: ProductId, id: PaymentTopUpId): Flow<TopUpStatus> {
        return hostApiInteractor.subscribeTopUpStatus(callingProductId, id)
    }

    override fun subscribePaymentStatus(
        callingProductId: ProductId,
        id: ProductPaymentRequestId,
    ): Flow<PaymentStatus> {
        return hostApiInteractor.subscribePaymentStatus(callingProductId, id)
    }

    override fun subscribeTheme(): Flow<ProductTheme> {
        return hostApiInteractor.subscribeTheme()
    }

    override suspend fun chainNodes(genesisHash: GenesisHash): Result<List<String>> {
        return hostApiInteractor.chainNodes(genesisHash)
    }

    override suspend fun chainSupported(genesisHash: GenesisHash): Result<Boolean> {
        return hostApiInteractor.chainSupported(genesisHash)
    }

    override suspend fun deriveEntropy(callingProductId: ProductId, key: ByteArray): Result<ByteArray> {
        return hostApiInteractor.deriveEntropy(callingProductId, key)
    }

    override suspend fun signPayload(signingRequestBody: SigningRequestBody.Transaction): Result<SignedTransaction.PayloadJson> {
        return openSigningScreen(signingRequestBody).map { it as SignedTransaction.PayloadJson }
    }

    override suspend fun signRaw(signingRequestBody: SigningRequestBody.Raw): Result<SignedTransaction.Raw> {
        return openSigningScreen(signingRequestBody).map { it as SignedTransaction.Raw }
    }

    override suspend fun signCreateTransaction(signingRequestBody: SigningRequestBody.CreateTransaction): Result<SignedTransaction.GeneralTransaction> {
        return openSigningScreen(signingRequestBody).map { it as SignedTransaction.GeneralTransaction }
    }

    override suspend fun signRawLegacy(signingRequestBody: SigningRequestBody.RawLegacy): Result<SignedTransaction.Raw> {
        return openSigningScreen(signingRequestBody).map { it as SignedTransaction.Raw }
    }

    override suspend fun signCreateTransactionLegacy(
        signingRequestBody: SigningRequestBody.CreateTransactionLegacy,
    ): Result<SignedTransaction.GeneralTransaction> {
        return openSigningScreen(signingRequestBody).map { it as SignedTransaction.GeneralTransaction }
    }

    private suspend fun openSigningScreen(signingRequestBody: SigningRequestBody): Result<SignedTransaction> {
        return callingProductIdProvider.getProductId().flatMap { productId ->
            hostApiInteractor.openSigningScreen(productId, signingRequestBody)
        }
    }
}
