@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi

import androidx.compose.ui.graphics.Color
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519SubstrateKeypairFactory
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.bandersnatch_crypto.ContextualAlias
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.GenesisHash
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ConnectionSecrets
import io.paritytech.polkadotapp.chains.multiNetwork.connection.saturateNodeUrls
import io.paritytech.polkadotapp.chains.multiNetwork.getChain
import io.paritytech.polkadotapp.chains.multiNetwork.getChainOrNull
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.wssNodes
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flowOfAll
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.design.theme.AppThemeSelector
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.PaymentStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TotalBalanceUseCase
import io.paritytech.polkadotapp.feature_products_api.domain.ProductRequestAccountResolver
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.AccountsProtocol
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.AllocatableResource
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.AllocationOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocatableResource
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocationOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.OnExistingAllowancePolicy
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ProductProofContext
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RegisteredRingVrfKey
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocation
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingVrfKeyDisclosure
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingVrfProof
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.VrfSignature
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.VrfTranscriptItem
import io.paritytech.polkadotapp.feature_products_api.domain.deriveEntropy.DeriveEntropyUseCase
import io.paritytech.polkadotapp.feature_products_api.domain.sponsoring.PreimageSubmitSponsoring
import io.paritytech.polkadotapp.feature_products_api.domain.sponsoring.StatementStoreSubmissionSponsoring
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.signing.SignedTransaction
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningAccount
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRequestBody
import io.paritytech.polkadotapp.feature_products_impl.domain.ProductAccountDerivationUseCase
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.allowance.AllowanceKeyUseCase
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.allowance.AllowanceResourceKind
import io.paritytech.polkadotapp.feature_products_impl.domain.notifications.NotificationId
import io.paritytech.polkadotapp.feature_products_impl.domain.notifications.ProductNotificationScheduler
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.ProductPaymentRequestId
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.RequestPaymentUseCase
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.spendableByProducts
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionGuard
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.RealProductPermissionRequester
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.handlers.DeviceCapabilityPermissionHandler
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.DeviceCapabilityType
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.PermissionDecision
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermissionDeniedException
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.RemotePermissionRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.signTransaction.ProductSigningScreenLauncher
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.PaymentTopUpId
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.PaymentTopUpSource
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpService
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpStatus
import io.paritytech.polkadotapp.feature_statement_store_api.data.Statement
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementStoreService
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementsPage
import io.paritytech.polkadotapp.feature_statement_store_api.data.TopicFilter
import io.paritytech.polkadotapp.feature_statement_store_api.data.models.StatementStoreMessageProof
import io.paritytech.polkadotapp.feature_statement_store_api.domain.StatementStoreMessageProver
import io.paritytech.polkadotapp.feature_transaction_storage_api.domain.TransactionStorageService
import io.paritytech.polkadotapp.feature_transactions.api.data.origins.SignedOrigins
import io.paritytech.polkadotapp.feature_transactions.api.data.origins.signedOriginSr25519PrivateKey
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.UsernameOfAccountUseCase
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapNotNull
import timber.log.Timber
import javax.inject.Inject
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

class HostApiInteractor @Inject constructor(
    private val deriveEntropyUseCase: DeriveEntropyUseCase,
    private val productAccountDerivationUseCase: ProductAccountDerivationUseCase,
    private val usernameOfAccountUseCase: UsernameOfAccountUseCase,
    private val chainRegistry: ChainRegistry,
    private val connectionSecrets: ConnectionSecrets,
    private val permissionGuard: ProductPermissionGuard,
    private val permissionRequester: RealProductPermissionRequester,
    private val deviceCapabilityPermissionHandler: DeviceCapabilityPermissionHandler,
    private val ipfsContentLookup: IpfsContentLookup,
    private val statementStoreService: StatementStoreService,
    private val statementStoreMessageProverFactory: StatementStoreMessageProver.Factory,
    private val productNotificationPublisher: ProductNotificationPublisher,
    private val productNotificationScheduler: ProductNotificationScheduler,
    private val totalBalanceUseCase: TotalBalanceUseCase,
    private val requestPaymentUseCase: RequestPaymentUseCase,
    private val topUpService: TopUpService,
    private val signedOrigins: SignedOrigins,
    private val accountsProtocol: AccountsProtocol,
    private val allowanceKeyUseCase: AllowanceKeyUseCase,
    private val transactionStorageService: TransactionStorageService,
    private val preimageSubmitSponsoring: PreimageSubmitSponsoring,
    private val statementStoreSubmissionSponsoring: StatementStoreSubmissionSponsoring,
    private val appThemeSelector: AppThemeSelector,
    private val productRequestAccountResolver: ProductRequestAccountResolver,
    private val productSigningScreenLauncher: ProductSigningScreenLauncher,
) {
    fun subscribeTheme(): Flow<ProductTheme> {
        return appThemeSelector.selectedTheme.map(PolkadotAppTheme::toProductTheme)
    }

    suspend fun accountGet(callingProductId: ProductId, productAccountId: ProductAccountId): Result<ProductAccountResult> {
        val permission = ProductPermission.AccountAccess(productAccountId.productId)
        if (!permissionGuard.requestPermission(callingProductId, permission)) {
            return Result.failure(ProductPermissionDeniedException(permission))
        }

        return productAccountDerivationUseCase.deriveAccountId(productAccountId).map { publicKey ->
            ProductAccountResult(
                publicKey = publicKey.value.toHexString(withPrefix = true),
            )
        }
    }

    suspend fun getUserId(callingProductId: ProductId): Result<GetUserIdResult> {
        val username = usernameOfAccountUseCase.getUsername()
            .onFailure { Timber.w(it, "host_get_user_id: failed to read username") }
            .getOrNull()
            ?: return Result.failure(UserNotConnectedException)

        val permission = ProductPermission.UserIdentityAccess
        if (!permissionGuard.requestPermission(callingProductId, permission)) {
            return Result.failure(ProductPermissionDeniedException(permission))
        }

        return Result.success(GetUserIdResult(primaryUsername = username.username.getDisplayUsername()))
    }

    suspend fun chainNodes(genesisHash: GenesisHash): Result<List<String>> = runCatching {
        val chain = chainRegistry.getChain(genesisHash)
        chain.nodes.wssNodes()
            .saturateNodeUrls(connectionSecrets)
            .map { it.saturatedUrl }
    }

    suspend fun chainSupported(genesisHash: GenesisHash): Result<Boolean> = runCatching {
        chainRegistry.getChainOrNull(genesisHash) != null
    }

    suspend fun getLegacyAccounts(): Result<List<LegacyAccountResult>> = runCatching {
        // We support no legacy accounts so far
        emptyList()
    }

    /**
     * Opens the signing confirmation screen for [signingRequestBody] and awaits the user's decision.
     * For legacy requests the supplied account is reverse-resolved first; an unresolved account is
     * rejected before any UI is shown.
     */
    suspend fun openSigningScreen(
        callingProductId: ProductId,
        signingRequestBody: SigningRequestBody,
    ): Result<SignedTransaction> {
        return signingAccountFor(signingRequestBody).flatMap { signingAccount ->
            productSigningScreenLauncher.awaitDecision(
                requesterName = callingProductId.value,
                requesterIconUrl = "",
                signingRequestBody = signingRequestBody,
                signingAccount = signingAccount,
            )
        }
    }

    /**
     * RFC-0023 `host_account_sign_vrf`. Authorization is a per-call user confirmation, owned by
     * [AccountsProtocol.signVrf] so the SSO Account-Holder path shares it.
     */
    suspend fun signVrf(
        callingProductId: ProductId,
        account: ProductAccountId,
        transcriptLabel: ByteArray,
        items: List<VrfTranscriptItem>,
    ): Result<VrfSignature> {
        return accountsProtocol.signVrf(callingProductId, account, transcriptLabel, items)
    }

    private suspend fun signingAccountFor(signingRequestBody: SigningRequestBody): Result<SigningAccount> {
        return when (signingRequestBody) {
            is SigningRequestBody.ProductAccountSigning -> Result.success(SigningAccount.Product(signingRequestBody.account))
            is SigningRequestBody.LegacyAccountSigning -> productRequestAccountResolver.resolve(signingRequestBody.account)
        }
    }

    suspend fun lookupPreimage(hash: ByteArray): Result<ByteArray> {
        return ipfsContentLookup.lookupRawHash(hash)
    }

    suspend fun submitPreimage(callingProductId: ProductId, data: ByteArray): Result<String> {
        val permission = ProductPermission.RemotePermission.PreimageSubmitAccess
        if (!permissionGuard.consumePermission(callingProductId, permission)) {
            return Result.failure(ProductPermissionDeniedException(permission))
        }

        return preimageSubmitSponsoring.sponsorPreimageSubmit(callingProductId, data.size.bytes)
            .flatMap { key -> signedOrigins.signedOriginSr25519PrivateKey(key.bytes) }
            .flatMap { origin -> transactionStorageService.store(data, origin) }
    }

    suspend fun createStatementProof(
        callingProductId: ProductId,
        productAccountId: ProductAccountId,
        statementBody: Statement.Body,
    ): Result<StatementStoreMessageProof> {
        val permission = ProductPermission.AccountAccess(productAccountId.productId)
        if (!permissionGuard.requestPermission(callingProductId, permission)) {
            return Result.failure(ProductPermissionDeniedException(permission))
        }

        return productAccountDerivationUseCase.deriveKeypair(productAccountId)
            .map(statementStoreMessageProverFactory::createKeyPairProver)
            .mapCatching { prover -> prover.generateMessageProof(statementBody) }
    }

    suspend fun createStatementProofAuthorized(
        callingProductId: ProductId,
        statementBody: Statement.Body,
    ): Result<StatementStoreMessageProof> {
        return statementStoreSubmissionSponsoring.ensureSponsorshipKey(callingProductId)
            .map { Sr25519SubstrateKeypairFactory.createKeypairFromSecret(it.bytes.value) }
            .map { keypair -> statementStoreMessageProverFactory.createKeyPairProver(keypair) }
            .mapCatching { prover -> prover.generateMessageProof(statementBody) }
    }

    suspend fun requestResourceAllocation(
        callingProductId: ProductId,
        resources: List<AllocatableResource>,
    ): Result<List<AllocationOutcome>> = runCatching {
        val apOutcomes = accountsProtocol.requestResourceAllocation(
            callingProduct = callingProductId,
            resources = resources.map { it.toApResource() },
            onExisting = OnExistingAllowancePolicy.INCREASE,
        )
        apOutcomes.forEach { allowanceKeyUseCase.persistIfAllocated(callingProductId, it) }
        apOutcomes.map { it.toTrUApiOutcome() }
    }.logFailure("requestResourceAllocation failed")

    suspend fun publishNotification(
        callingProductId: ProductId,
        text: String,
        deeplink: String?,
        scheduledAt: Instant?,
    ): Result<NotificationId> {
        val permission = ProductPermission.DeviceCapability(DeviceCapabilityType.Notifications)
        if (!permissionGuard.consumePermission(callingProductId, permission)) {
            return Result.failure(ProductPermissionDeniedException(permission))
        }

        return publishNotificationAuthorized(callingProductId, text, deeplink, scheduledAt)
    }

    suspend fun cancelNotification(callingProductId: ProductId, notificationId: NotificationId): Result<Unit> {
        val permission = ProductPermission.DeviceCapability(DeviceCapabilityType.Notifications)
        if (!permissionGuard.consumePermission(callingProductId, permission)) {
            return Result.failure(ProductPermissionDeniedException(permission))
        }

        return cancelNotificationAuthorized(callingProductId, notificationId)
    }

    suspend fun cancelNotificationAuthorized(callingProductId: ProductId, notificationId: NotificationId): Result<Unit> {
        return productNotificationScheduler.cancel(callingProductId, notificationId)
    }

    suspend fun requestDevicePermission(callingProductId: ProductId, capability: DeviceCapabilityType): Result<Boolean> {
        val permission = ProductPermission.DeviceCapability(capability)
        return Result.success(permissionGuard.requestPermission(callingProductId, permission))
    }

    suspend fun requestRemotePermissions(
        callingProductId: ProductId,
        requests: List<RemotePermissionRequest>,
    ): Result<Boolean> = runCatching {
        val permissions = requests.flatMap { it.toDomainPermissions() }
        permissionGuard.requestPermissionsBatched(callingProductId, permissions)
    }

    suspend fun requestDevicePermissionDecision(
        callingProductId: ProductId,
        capability: DeviceCapabilityType,
    ): Result<PermissionDecision> = runCatching {
        val permission = ProductPermission.DeviceCapability(capability)
        val decision = permissionRequester.prompt(callingProductId, permission)
        if (decision != PermissionDecision.Deny &&
            !deviceCapabilityPermissionHandler.requestOsPermissionIfNeeded(capability)
        ) {
            throw ProductPermissionDeniedException(permission)
        }
        decision
    }

    suspend fun requestRemotePermissionDecision(
        callingProductId: ProductId,
        request: RemotePermissionRequest,
    ): Result<PermissionDecision> = runCatching {
        permissionRequester.promptBatched(callingProductId, request.toDomainPermissions())
    }

    suspend fun allowWebRtcAccess(callingProductId: ProductId): Result<Boolean> {
        val permission = ProductPermission.RemotePermission.WebRtcAccess
        return Result.success(permissionGuard.consumePermission(callingProductId, permission))
    }

    suspend fun submitStatement(callingProductId: ProductId, statement: Statement): Result<Unit> {
        val permission = ProductPermission.RemotePermission.StatementSubmitAccess
        if (!permissionGuard.consumePermission(callingProductId, permission)) {
            return Result.failure(ProductPermissionDeniedException(permission))
        }

        val signer = statement.proof.publicKey.toDataByteArray()
        return statementStoreSubmissionSponsoring.validateSponsorship(callingProductId, signer)
            .flatMap { statementStoreService.submitStatement(statement) }
    }

    fun subscribeStatements(filter: TopicFilter): Flow<StatementsPage> {
        return statementStoreService.subscribeStatements(filter)
            .mapNotNull { it.getOrNull() }
    }

    suspend fun requestPayment(
        callingProductId: ProductId,
        id: ProductPaymentRequestId,
        amount: Balance,
        destination: AccountId,
    ): Result<Unit> {
        return requestPaymentUseCase.requestPayment(callingProductId, id, amount, destination)
    }

    suspend fun publishNotificationAuthorized(
        callingProductId: ProductId,
        text: String,
        deeplink: String?,
        scheduledAt: Instant?
    ): Result<NotificationId> {
        return if (scheduledAt == null) {
            val id = NotificationId.generate(callingProductId)
            productNotificationPublisher.publishNotification(id.value, text, deeplink)
            Result.success(id)
        } else {
            productNotificationScheduler.schedule(callingProductId, text, deeplink, scheduledAt)
        }
    }

    fun subscribePaymentStatus(callingProductId: ProductId, id: ProductPaymentRequestId): Flow<PaymentStatus> {
        return requestPaymentUseCase.subscribeStatus(callingProductId, id)
    }

    /**
     * Registers the top-up and returns; it is followed through [subscribeTopUpStatus], not waited on here.
     * The id is the product's, so a product killed mid-top-up can still name the operation it started.
     */
    suspend fun topUp(
        callingProductId: ProductId,
        id: PaymentTopUpId,
        amount: Balance,
        source: PaymentTopUpSource,
    ): Result<Unit> {
        return topUpService.start(callingProductId, id, amount, source)
    }

    fun subscribeTopUpStatus(callingProductId: ProductId, id: PaymentTopUpId): Flow<TopUpStatus> {
        return topUpService.status(callingProductId, id)
    }

    suspend fun deriveEntropy(callingProductId: ProductId, key: ByteArray): Result<ByteArray> {
        return deriveEntropyUseCase.deriveEntropy(callingProductId, key)
    }

    fun subscribePaymentBalance(callingProductId: ProductId): Flow<PaymentBalance> = flowOfAll {
        val permission = ProductPermission.BalanceAccess
        if (!permissionGuard.requestPermission(callingProductId, permission)) {
            throw ProductPermissionDeniedException(permission)
        }

        totalBalanceUseCase.subscribeTotalBalance()
            .mapNotNull { it.getOrNull() }
            // What a payment request may spend, so a product never sees an amount it cannot ask for.
            .map { PaymentBalance(available = it.spendableByProducts()) }
    }

    suspend fun registerRingVrfKey(
        callingProductId: ProductId,
        index: DerivationIndex32,
        ring: RingLocation,
    ): Result<DataByteArray> {
        return accountsProtocol.registerRingVrfKey(callingProductId, index, ring)
    }

    suspend fun listRingVrfKeys(
        callingProductId: ProductId,
        owner: ProductId,
        disclosure: RingVrfKeyDisclosure,
    ): Result<List<RegisteredRingVrfKey>> {
        return accountsProtocol.listRingVrfKeys(callingProductId, owner, disclosure)
    }

    suspend fun ringVrfSign(
        callingProductId: ProductId,
        keyHandle: ProductAccountId,
        message: ByteArray,
    ): Result<ByteArray> {
        return accountsProtocol.ringVrfSign(callingProductId, keyHandle, message)
    }

    suspend fun accountGetAlias(
        callingProductId: ProductId,
        keyHandle: ProductAccountId,
        context: ProductProofContext,
        ring: RingLocation,
    ): Result<ContextualAlias> {
        return accountsProtocol.getContextualAlias(callingProductId, keyHandle, context, ring)
    }

    suspend fun accountCreateProof(
        callingProductId: ProductId,
        keyHandle: ProductAccountId,
        context: ProductProofContext,
        ring: RingLocation,
        message: ByteArray,
    ): Result<RingVrfProof> {
        return accountsProtocol.createProof(callingProductId, keyHandle, context, ring, message)
    }
}

data class ProductAccountResult(val publicKey: String)

data class LegacyAccountResult(val publicKey: String, val name: String?)

data class GetUserIdResult(val primaryUsername: String)

data class PaymentBalance(val available: Balance)

data class ProductTheme(val name: String, val variant: ThemeVariant)

enum class ThemeVariant { Light, Dark }

private fun PolkadotAppTheme.toProductTheme(): ProductTheme = ProductTheme(
    name = name.replaceFirstChar(Char::lowercaseChar),
    variant = if (colors().bg.surface.main.isLight) ThemeVariant.Light else ThemeVariant.Dark,
)

// Perceived-brightness (Rec. 601 luma) of the surface color
private val Color.isLight: Boolean
    get() = (0.299f * red + 0.587f * green + 0.114f * blue) > 0.5f

class AllowanceDeniedException(val kind: AllowanceResourceKind) :
    RuntimeException("allowance allocation rejected by user for $kind")

class AllowanceUnavailableException(val kind: AllowanceResourceKind) :
    RuntimeException("allowance not available for $kind")

private fun AllocatableResource.toApResource(): ApAllocatableResource = when (this) {
    AllocatableResource.BulletInAllowance -> ApAllocatableResource.BulletInAllowance
    AllocatableResource.StatementStoreAllowance -> ApAllocatableResource.StatementStoreAllowance
    is AllocatableResource.SmartContractAllowance -> ApAllocatableResource.SmartContractAllowance(dest)
    AllocatableResource.AutoSigning -> ApAllocatableResource.AutoSigning
}

private fun ApAllocationOutcome.toTrUApiOutcome(): AllocationOutcome = when (this) {
    is ApAllocationOutcome.Allocated -> AllocationOutcome.Allocated
    ApAllocationOutcome.Rejected -> AllocationOutcome.Rejected
    ApAllocationOutcome.NotAvailable -> AllocationOutcome.NotAvailable
}

/** Message must be literal "NotConnected" — load-bearing token matched by JS-side `handleGetUserId`. */
object UserNotConnectedException : RuntimeException("NotConnected")

private fun RemotePermissionRequest.toDomainPermissions(): List<ProductPermission.RemotePermission> = when (this) {
    is RemotePermissionRequest.Remote -> domains.map { ProductPermission.RemotePermission.NetworkAccess(it) }
    RemotePermissionRequest.WebRtc -> listOf(ProductPermission.RemotePermission.WebRtcAccess)
    RemotePermissionRequest.ChainSubmit -> listOf(ProductPermission.RemotePermission.ChainSubmitAccess)
    RemotePermissionRequest.StatementSubmit -> listOf(ProductPermission.RemotePermission.StatementSubmitAccess)
    RemotePermissionRequest.PreimageSubmit -> listOf(ProductPermission.RemotePermission.PreimageSubmitAccess)
}
