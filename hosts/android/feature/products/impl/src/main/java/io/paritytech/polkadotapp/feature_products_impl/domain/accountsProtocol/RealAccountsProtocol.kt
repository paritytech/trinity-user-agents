package io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.bandersnatch_crypto.ContextualAlias
import io.paritytech.polkadotapp.bandersnatch_crypto.aliasInContext
import io.paritytech.polkadotapp.bandersnatch_crypto.sign
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.kilobytes
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.mapErrorNotInstance
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.BandersnatchSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.getContextualAlias
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_members_api.domain.MembershipProver
import io.paritytech.polkadotapp.feature_members_api.domain.RingVrfProofError
import io.paritytech.polkadotapp.feature_members_api.domain.model.MemberSource
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.AccountsProtocol
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocatableResource
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocationOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.CreateProofError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.GetAliasError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ListRingVrfKeysError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.LocatedRing
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.MembersRingLocator
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.OnExistingAllowancePolicy
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ProductProofContext
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RegisterRingVrfKeyError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RegisteredRingVrfKey
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocation
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocationError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingVrfKeyDisclosure
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingVrfProof
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingVrfSignError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.SignVrfError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.VrfSignature
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.VrfTranscriptItem
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.signing.SignedTransaction
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningAccount
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRequestBody
import io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol.registry.RingVrfKeyRegistry
import io.paritytech.polkadotapp.feature_products_impl.domain.crossProductProof.CrossProductProofContext
import io.paritytech.polkadotapp.feature_products_impl.domain.crossProductProof.CrossProductProofContextHolder
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionGuard
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.domain.resourceAllocationRequest.ResourceAllocationRequestContext
import io.paritytech.polkadotapp.feature_products_impl.domain.resourceAllocationRequest.ResourceAllocationRequestContextHolder
import io.paritytech.polkadotapp.feature_products_impl.domain.signTransaction.ProductSigningScreenLauncher
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import java.util.concurrent.CancellationException
import javax.inject.Inject

class RealAccountsProtocol @Inject constructor(
    private val contextHolder: ResourceAllocationRequestContextHolder,
    private val productsRouter: ProductsRouter,
    private val membersRingLocator: MembersRingLocator,
    private val membershipProver: MembershipProver,
    private val bandersnatchSecretsStorage: BandersnatchSecretsStorage,
    private val ringVrfKeyRegistry: RingVrfKeyRegistry,
    private val ringVrfKeySource: RingVrfKeySource,
    private val permissionGuard: ProductPermissionGuard,
    private val crossProductProofContextHolder: CrossProductProofContextHolder,
    private val productSigningScreenLauncher: ProductSigningScreenLauncher,
) : AccountsProtocol {
    override suspend fun registerRingVrfKey(
        callingProductId: ProductId,
        index: DerivationIndex32,
        ring: RingLocation,
    ): Result<DataByteArray> {
        // Ownership is the caller, never a parameter, so registration needs no capability gate.
        return ringVrfKeyRegistry.register(callingProductId, index, ring)
            .mapErrorNotInstance<_, RegisterRingVrfKeyError> { it.toRegisterError() }
    }

    override suspend fun listRingVrfKeys(
        callingProductId: ProductId,
        owner: ProductId,
        disclosure: RingVrfKeyDisclosure,
    ): Result<List<RegisteredRingVrfKey>> {
        if (!ensureOwnerAccessApproved(callingProductId, owner)) {
            return Result.failure(ListRingVrfKeysError.Rejected)
        }

        return ringVrfKeyRegistry.list(owner)
            .map { entries -> entries.applyDisclosure(disclosure) }
            .mapErrorNotInstance<_, ListRingVrfKeysError> { it.toListError() }
    }

    override suspend fun getContextualAlias(
        callingProductId: ProductId,
        keyHandle: ProductAccountId,
        context: ProductProofContext,
        ring: RingLocation,
    ): Result<ContextualAlias> {
        if (!ensureAliasApproved(callingProductId, context)) {
            return Result.failure(GetAliasError.Rejected)
        }

        return ringVrfKeySource.resolveMember(keyHandle, ring).mapCatching { member ->
            member.contextualAlias(context.productContextBytes())
        }.mapErrorNotInstance<_, GetAliasError> { it.toGetAliasError() }
    }

    override suspend fun createProof(
        callingProductId: ProductId,
        keyHandle: ProductAccountId,
        context: ProductProofContext,
        ring: RingLocation,
        message: ByteArray,
    ): Result<RingVrfProof> {
        if (!awaitCrossProductProofApproval(callingProductId, context, message)) {
            return Result.failure(CreateProofError.Rejected)
        }

        val contextBytes = context.productContextBytes()

        return ringVrfKeySource.resolveMember(keyHandle, ring)
            .mapErrorNotInstance<_, CreateProofError> { it.toCreateProofError() }
            .flatMap { member -> createProofForMember(member, ring, contextBytes, message) }
    }

    /**
     * TODO RFC-0024 § "Using a foreign key means trusting the caller": a signature is a bearer token
     * for the key, so the spec gates this on the owner's manifest allowlist with no prompt fallback.
     * Until the manifest RFC lands we keep create_proof's one-time cross-product prompt.
     */
    override suspend fun ringVrfSign(
        callingProductId: ProductId,
        keyHandle: ProductAccountId,
        message: ByteArray,
    ): Result<ByteArray> {
        if (!awaitCrossProductSignApproval(callingProductId, keyHandle, message)) {
            return Result.failure(RingVrfSignError.Rejected)
        }

        return ringVrfKeySource.resolveEntropy(keyHandle)
            .mapCatching { entropy -> entropy.sign(message) }
            .mapErrorNotInstance<_, RingVrfSignError> { it.toRingVrfSignError() }
    }

    override suspend fun signVrf(
        callingProductId: ProductId,
        account: ProductAccountId,
        transcriptLabel: ByteArray,
        items: List<VrfTranscriptItem>,
    ): Result<VrfSignature> {
        validateTranscriptSize(transcriptLabel, items)?.let { return Result.failure(it) }

        val request = SigningRequestBody.SignVrf(
            account = account,
            transcriptLabel = transcriptLabel,
            items = items,
        )

        return productSigningScreenLauncher.awaitDecision(
            requesterName = callingProductId.value,
            requesterIconUrl = "",
            signingRequestBody = request,
            signingAccount = SigningAccount.Product(account),
        )
            .mapErrorNotInstance<_, SignVrfError> { it.toSignVrfError() }
            .map { signed -> (signed as SignedTransaction.Vrf).signature }
    }

    // RFC-0023 § Implementation notes: a host must bound the transcript a caller can hand it. The
    // check runs before the confirmation so the user is never shown an unbounded request.
    private fun validateTranscriptSize(
        transcriptLabel: ByteArray,
        items: List<VrfTranscriptItem>,
    ): SignVrfError? {
        if (items.size > MAX_TRANSCRIPT_ITEMS) {
            return SignVrfError.TranscriptTooLarge(
                "VRF transcript has ${items.size} items, at most $MAX_TRANSCRIPT_ITEMS are allowed"
            )
        }

        val totalSize = transcriptLabel.size + items.sumOf { it.label.value.size + it.value.value.size }
        if (totalSize > MAX_TRANSCRIPT_SIZE.inWholeBytes) {
            return SignVrfError.TranscriptTooLarge(
                "VRF transcript is $totalSize bytes, at most ${MAX_TRANSCRIPT_SIZE.inWholeBytes} are allowed"
            )
        }

        return null
    }

    // Deriving an alias under another product's context reuses the persisted account-access grant.
    // AccountAccessPermissionHandler short-circuits same-product requests, so no guard is needed here.
    private suspend fun ensureAliasApproved(
        callingProductId: ProductId,
        context: ProductProofContext,
    ): Boolean {
        return permissionGuard.requestPermission(
            callingProductId,
            ProductPermission.AccountAccess(context.productId.value),
        )
    }

    // Listing another product's registry is an ordinary read, so it reuses the persisted
    // account-access grant rather than a one-time prompt.
    private suspend fun ensureOwnerAccessApproved(callingProductId: ProductId, owner: ProductId): Boolean {
        return permissionGuard.requestPermission(callingProductId, ProductPermission.AccountAccess(owner.value))
    }

    private fun List<RegisteredRingVrfKey>.applyDisclosure(
        disclosure: RingVrfKeyDisclosure,
    ): List<RegisteredRingVrfKey> = when (disclosure) {
        RingVrfKeyDisclosure.PUBLIC_KEY -> this
        RingVrfKeyDisclosure.ANONYMIZED -> map { entry -> entry.copy(publicKey = null) }
    }

    private suspend fun awaitCrossProductSignApproval(
        callingProductId: ProductId,
        keyHandle: ProductAccountId,
        message: ByteArray,
    ): Boolean {
        val owner = ProductId.fromStoredValue(keyHandle.productId)
        if (owner == callingProductId) return true

        return awaitCrossProductApproval(callingProductId, owner, keyHandle.index, message)
    }

    // A cross-product proof is a one-time action: always prompt with the action details, never persist.
    private suspend fun awaitCrossProductProofApproval(
        callingProductId: ProductId,
        context: ProductProofContext,
        message: ByteArray,
    ): Boolean {
        if (context.productId == callingProductId) return true

        return awaitCrossProductApproval(callingProductId, context.productId, context.suffix, message)
    }

    private suspend fun awaitCrossProductApproval(
        callingProductId: ProductId,
        onBehalfOf: ProductId,
        suffix: DerivationIndex32,
        message: ByteArray,
    ): Boolean {
        val proofContext = CrossProductProofContext(
            callingProduct = callingProductId,
            onBehalfOf = onBehalfOf,
            suffix = suffix,
            message = message.toDataByteArray(),
        )
        crossProductProofContextHolder.set(proofContext)
        productsRouter.openCrossProductProofPrompt()
        return proofContext.awaitDecision() is CrossProductProofContext.Decision.Approved
    }

    private suspend fun MemberSource.contextualAlias(context: BandersnatchContext): ContextualAlias = when (this) {
        is MemberSource.Account -> bandersnatchSecretsStorage.getContextualAlias(metaId, context)
        is MemberSource.Entropy -> ContextualAlias(context, bandersnatchEntropy.aliasInContext(context))
    }

    // The locator no longer picks the key — it only resolves the ring the prover needs.
    private suspend fun createProofForMember(
        member: MemberSource,
        ring: RingLocation,
        contextBytes: BandersnatchContext,
        message: ByteArray,
    ): Result<RingVrfProof> {
        return membersRingLocator.locateRing(ring)
            .mapErrorNotInstance<_, CreateProofError> { it.toCreateProofError() }
            .flatMap { located -> createProofForRing(member, located, contextBytes, message) }
    }

    private suspend fun createProofForRing(
        member: MemberSource,
        located: LocatedRing,
        contextBytes: BandersnatchContext,
        message: ByteArray,
    ): Result<RingVrfProof> {
        return membershipProver.createRingVrfProof(
            member = member,
            message = message,
            context = contextBytes,
            chainId = located.chainId,
            collectionId = located.collectionId,
        ).map { membershipProof ->
            RingVrfProof(
                proof = membershipProof.proof,
                contextualAlias = ContextualAlias(contextBytes, membershipProof.alias),
                ringIndex = membershipProof.ringIndex,
                ringRevision = membershipProof.ringRevision,
            )
        }.mapErrorNotInstance<_, CreateProofError> { it.toCreateProofError() }
    }

    // get_alias is derive-only (no membership proof), so it never reports NotMember; RingNotFound
    // comes from ring resolution, shared with create_proof via the locator's CreateProofError.
    private fun Throwable.toGetAliasError(): GetAliasError = when (this) {
        is RingLocationError.RingNotFound -> GetAliasError.RingNotFound
        is RingVrfKeyError.KeyNotRegistered -> GetAliasError.KeyNotRegistered
        is RingVrfKeyError.KeyNotInRing -> GetAliasError.KeyNotInRing
        else -> GetAliasError.Unknown(message ?: "Failed to derive contextual alias")
    }

    private fun Throwable.toRegisterError(): RegisterRingVrfKeyError = when (this) {
        is RingLocationError.RingNotFound -> RegisterRingVrfKeyError.RingNotFound
        else -> RegisterRingVrfKeyError.Unknown(message ?: "Failed to register ring VRF key")
    }

    private fun Throwable.toListError(): ListRingVrfKeysError =
        ListRingVrfKeysError.Unknown(message ?: "Failed to list ring VRF keys")

    private fun Throwable.toRingVrfSignError(): RingVrfSignError = when (this) {
        is RingVrfKeyError.KeyNotRegistered -> RingVrfSignError.KeyNotRegistered
        else -> RingVrfSignError.Unknown(message ?: "Failed to sign with the ring VRF key")
    }

    private fun Throwable.toSignVrfError(): SignVrfError = when (this) {
        // ProductSigningContext reports a declined confirmation as a CancellationException.
        is CancellationException -> SignVrfError.Rejected
        else -> SignVrfError.Unknown(message ?: "Failed to sign VRF")
    }

    private fun Throwable.toCreateProofError(): CreateProofError = when (this) {
        is RingLocationError.RingNotFound -> CreateProofError.RingNotFound
        is RingVrfKeyError.KeyNotRegistered -> CreateProofError.KeyNotRegistered
        is RingVrfKeyError.KeyNotInRing -> CreateProofError.KeyNotInRing
        is RingVrfProofError.NotMember -> CreateProofError.NotMember
        is RingVrfProofError.RingNotFound -> CreateProofError.RingNotFound
        is RingVrfProofError.DataFetchFailed -> CreateProofError.Unknown(message ?: "Failed to fetch ring data")
        else -> CreateProofError.Unknown(message ?: "Failed to create proof")
    }

    /**
     * The prompt both confirms and performs the allocation, so that its progress is reported on the screen the user
     * is looking at - this call only hands the request over and waits for what the prompt made of it.
     */
    override suspend fun requestResourceAllocation(
        callingProduct: ProductId,
        resources: List<ApAllocatableResource>,
        onExisting: OnExistingAllowancePolicy,
    ): List<ApAllocationOutcome> {
        if (resources.isEmpty()) return emptyList()

        val context = ResourceAllocationRequestContext(
            productId = callingProduct,
            resources = resources,
            onExisting = onExisting,
        )
        contextHolder.set(context)
        productsRouter.openResourceAllocationRequestPrompt()

        return context.awaitOutcomes()
    }
}

private const val MAX_TRANSCRIPT_ITEMS = 32
private val MAX_TRANSCRIPT_SIZE = 8.kilobytes
