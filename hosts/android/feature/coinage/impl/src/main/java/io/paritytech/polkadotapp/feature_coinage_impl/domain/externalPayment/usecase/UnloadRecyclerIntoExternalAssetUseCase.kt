package io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.usecase

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.AsTuple
import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchAlias
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchPublicKey
import io.paritytech.polkadotapp.bandersnatch_crypto.aliasInContext
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.network.rpc.RpcCalls
import io.paritytech.polkadotapp.chains.util.EncodedArguments.Companion.autoEncodedArgs
import io.paritytech.polkadotapp.chains.util.amountFromPlanks
import io.paritytech.polkadotapp.chains.util.call
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.CoinageBalanceConversionContext
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.VoucherAllocator
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.balance
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.totalBalance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstanceId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.isInRecycler
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.recyclerLocationOrThrow
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageInput
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionRequest
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionState
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinAmountBreakdownUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageBalanceConverterUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.data.config.CoinageInstanceIdProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.VoucherRingDerivation
import io.paritytech.polkadotapp.feature_coinage_impl.data.helpers.FreeUnloadTokenResolver
import io.paritytech.polkadotapp.feature_coinage_impl.data.helpers.UnloadTokenResolverFactory
import io.paritytech.polkadotapp.feature_coinage_impl.data.helpers.createForCollection
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.RecyclerProofDataProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.VoucherRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.signer.context.CoinageSigningContextProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.signer.origins.CoinageTransactionOrigins
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogD
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogE
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogI
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogW
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.UnloadQuotaTracker
import io.paritytech.polkadotapp.feature_members_api.data.model.RingRevision
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleMembershipProver
import io.paritytech.polkadotapp.feature_people_api.domain.PrecomputedPersonMembershipProver
import io.paritytech.polkadotapp.feature_people_api.domain.useCase.ActivePeopleCollectionUseCase
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_transactions.api.data.EnrichedSendableExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.transformWhile
import kotlinx.serialization.Serializable
import javax.inject.Inject

/**
 * Unloads a set of recycler vouchers into an external-asset balance on the given destination
 * account. When [surplus] is zero, dispatches Coinage.unload_recycler_into_external_asset per
 * (exponent, recyclerIndex) group. When non-zero, picks a single group that can carry the
 * surplus and dispatches Coinage.unload_recycler_into_external_asset_and_loaded_coins for it,
 * folding the surplus back into freshly-minted vouchers in the same call.
 */
interface UnloadRecyclerIntoExternalAssetUseCase {
    /**
     * Registers one transaction per recycler group under [groupId] and submits them. Returns once they are
     * all registered, not once they have executed: what became of each is the ledger's answer, and the caller
     * reads it from [groupId] rather than from here — a group is never one verdict.
     *
     * A group that already holds entries was submitted by an earlier attempt and is left alone, so a crash
     * between registration and the caller learning of it costs nothing.
     */
    suspend fun initiateUnload(
        vouchers: List<RecyclerVoucher>,
        destination: AccountId,
        surplus: Balance,
        groupId: CoinageOperationGroupId,
    ): Result<Unit>

    /**
     * What became of the unload registered under [groupId], as one answer.
     *
     * Emits until every transaction is decided. The caller does not have to know that an unload is several
     * transactions — only whether the destination got what it was promised.
     */
    fun subscribeUnloadStatus(groupId: CoinageOperationGroupId): Flow<ExternalUnloadStatus>
}

/** [PartialSuccess] is not a failure: money did move, just not all of it. */
sealed interface ExternalUnloadStatus {
    data object Submitted : ExternalUnloadStatus

    data object FinalizedSuccess : ExternalUnloadStatus

    /** [claimed] is what reached the destination, net of any surplus folded back into new vouchers. */
    data class PartialSuccess(val executed: Int, val total: Int, val claimed: Balance) : ExternalUnloadStatus

    data object Failed : ExternalUnloadStatus
}

class RealUnloadRecyclerIntoExternalAssetUseCase @Inject constructor(
    private val rpcCalls: RpcCalls,
    private val extrinsicService: ExtrinsicService,
    private val originFactory: CoinageTransactionOrigins,
    private val coinageSigningContextProvider: CoinageSigningContextProvider,
    private val voucherRingDerivation: VoucherRingDerivation,
    private val recyclerProofDataProvider: RecyclerProofDataProvider,
    private val activePeopleCollectionUseCase: ActivePeopleCollectionUseCase,
    private val unloadTokenResolverFactory: UnloadTokenResolverFactory,
    private val chainRegistry: ChainRegistry,
    private val voucherRepository: VoucherRepository,
    private val transactionService: CoinageTransactionService,
    private val voucherAllocator: VoucherAllocator,
    private val coinAmountBreakdownUseCase: CoinAmountBreakdownUseCase,
    private val coinageBalanceConverterUseCase: CoinageBalanceConverterUseCase,
    private val peopleMembershipProver: PeopleMembershipProver,
    private val quotaTracker: UnloadQuotaTracker,
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
    private val coinageInstanceIdProvider: CoinageInstanceIdProvider,
) : UnloadRecyclerIntoExternalAssetUseCase {
    override suspend fun initiateUnload(
        vouchers: List<RecyclerVoucher>,
        destination: AccountId,
        surplus: Balance,
        groupId: CoinageOperationGroupId,
    ): Result<Unit> {
        coinageLogI("Unload starting group=${groupId.value} vouchers=${vouchers.size} surplus=$surplus")

        val alreadySubmitted = transactionService.getOperationGroupStatuses(groupId)
            .getOrElse { return Result.failure(it) }

        // Before validating: once registered, the vouchers are locked or spent, so a rejoining caller no longer
        // sees them as unloadable.
        if (alreadySubmitted.isNotEmpty()) {
            coinageLogD("Unload already submitted group=${groupId.value} transactions=${alreadySubmitted.size}")
            return Result.success(Unit)
        }

        validateInputs(vouchers)?.let {
            coinageLogE("Unload rejected group=${groupId.value}: ${it.message}")
            return Result.failure(it)
        }

        val chain = chainRegistry.getChain(chainAssetProvider.chainId())

        return coinageBalanceConverterUseCase.create()
            .flatMap { balanceContext -> prepareGroups(chain, vouchers, surplus, balanceContext) }
            .onSuccess { prepared -> coinageLogD("Unload prepared group=${groupId.value} recyclerGroups=${prepared.groups.size}") }
            .flatMap { prepared -> submitGroups(chain, prepared, destination, groupId) }
    }

    private fun validateInputs(vouchers: List<RecyclerVoucher>): Throwable? = when {
        vouchers.isEmpty() -> IllegalArgumentException("UnloadRecyclerIntoExternalAsset.emptyVouchers")
        vouchers.any { !it.isInRecycler() } -> IllegalArgumentException("UnloadRecyclerIntoExternalAsset.missingRecyclerInfo")
        else -> null
    }

    private suspend fun prepareGroups(
        chain: Chain,
        vouchers: List<RecyclerVoucher>,
        surplus: Balance,
        context: CoinageBalanceConversionContext
    ): Result<UnloadGroups> {
        return with(context) {
            prepareGroups(chain, vouchers, surplus)
        }
    }

    context(coinageContext: CoinageBalanceConversionContext)
    private suspend fun prepareGroups(
        chain: Chain,
        vouchers: List<RecyclerVoucher>,
        surplus: Balance,
    ): Result<UnloadGroups> {
        val grouped = vouchers.groupByRecycler()
        val peopleCollection = activePeopleCollectionUseCase.getActivePeopleCollection()
        val resolvedTokens = unloadTokenResolverFactory
            .createForCollection(peopleCollection)
            .resolve(chain.id, grouped.size)
        val pinnedBlockHash = rpcCalls.getBlockHash(chain.id)

        return recyclerProofDataProvider
            .getRecyclerRevisions(chain.id, grouped.keys, pinnedBlockHash)
            .logFailure("Failed to get recycler revisions")
            .flatMap { revisions ->
                resolveMixedSetup(grouped, surplus).map { mintedSetup ->
                    UnloadGroups(
                        pinnedBlockHash = pinnedBlockHash,
                        peopleCollection = peopleCollection,
                        groups = buildGroups(
                            grouped = grouped,
                            resolvedTokens = resolvedTokens,
                            revisions = revisions,
                            mintedSetup = mintedSetup,
                            surplus = surplus,
                        ),
                    )
                }
            }
    }

    context(coinageContext: CoinageBalanceConversionContext)
    private suspend fun resolveMixedSetup(
        grouped: Map<RecyclerKey, List<RecyclerVoucher>>,
        surplus: Balance,
    ): Result<MintedSetup?> {
        if (surplus.isZero()) return Result.success(null)

        val group = grouped.entries
            .firstOrNull { (key, voucherList) -> key.exponent.balance() * voucherList.size >= surplus }
            ?: return Result.failure(IllegalStateException("UnloadRecyclerIntoExternalAsset: no recycler group large enough to host surplus $surplus"))

        return allocateSurplusVouchers(surplus).map { MintedSetup(group.key, it) }
    }

    private suspend fun allocateSurplusVouchers(surplus: Balance): Result<List<RecyclerVoucher>> {
        val amount = chainAssetProvider.asset().amountFromPlanks(surplus)

        return coinAmountBreakdownUseCase.createCoinAmountBreakdown()
            .mapCatching { it.breakdown(amount) }
            .flatMap { voucherAllocator.allocateAll(it) }
    }

    context(coinageContext: CoinageBalanceConversionContext)
    private fun buildGroups(
        grouped: Map<RecyclerKey, List<RecyclerVoucher>>,
        resolvedTokens: List<FreeUnloadTokenResolver.ResolvedUnloadToken>,
        revisions: Map<RecyclerKey, RingRevision>,
        mintedSetup: MintedSetup?,
        surplus: Balance,
    ): List<UnloadGroup> = grouped.entries.mapIndexed { index, (key, voucherList) ->
        val mixedOutput = if (key == mintedSetup?.hostKey) {
            val groupTotal = key.exponent.balance() * voucherList.size
            MixedOutput(
                externalAssetAmount = groupTotal - surplus,
                newVouchers = mintedSetup.newVouchers,
            )
        } else {
            null
        }

        UnloadGroup(
            recyclerKey = key,
            vouchers = voucherList,
            resolvedUnloadToken = resolvedTokens[index],
            revision = revisions.getValue(key),
            mixedOutput = mixedOutput,
        )
    }

    /**
     * Every extrinsic is built before any is registered, so their nonces stay sequenced, and the whole set is
     * then registered as one unit. All or nothing matters here: a caller reading the group back as a single
     * outcome cannot tell half a registration from half an execution, and they mean opposite things.
     */
    private suspend fun submitGroups(
        chain: Chain,
        prepared: UnloadGroups,
        destination: AccountId,
        groupId: CoinageOperationGroupId,
    ): Result<Unit> = runCatching {
        val personProver = peopleMembershipProver.precomputeForMember(
            chainId = chain.id,
            peopleCollection = prepared.peopleCollection,
            at = prepared.pinnedBlockHash,
        ).getOrThrow()

        val transactions = prepared.groups.map { group ->
            CoinageTransactionRequest(
                extrinsic = buildGroupExtrinsic(chain, prepared, group, destination, personProver).getOrThrow(),
                inputs = group.vouchers.map { CoinageInput.Voucher(it.ringVrfKeyIndex) },
                outputs = group.mixedOutput?.newVouchers.orEmpty().map { OwnAsset.Voucher(it.ringVrfKeyIndex) },
                policy = null,
            )
        }

        coinageLogI("Unload registering group=${groupId.value} transactions=${transactions.size}")

        transactionService.submitTransactions(transactions, groupId).getOrThrow()

        // After submission, not after resolving: a token picked for a transaction that never left is still
        // there to be picked again.
        quotaTracker.noteUnloadsHappened(prepared.groups.size)
    }

    override fun subscribeUnloadStatus(groupId: CoinageOperationGroupId): Flow<ExternalUnloadStatus> = flow {
        val balanceContext = coinageBalanceConverterUseCase.create().getOrElse { throw it }

        val statuses = transactionService.subscribeOperationGroupStatuses(groupId).transformWhile { states ->
            val status = with(balanceContext) { states.toUnloadStatus() }
            logUnloadStatus(groupId, status)

            emit(status)
            states.isEmpty() || states.any { it.status.isLive }
        }

        emitAll(statuses)
    }

    private fun logUnloadStatus(groupId: CoinageOperationGroupId, status: ExternalUnloadStatus) {
        val group = groupId.value

        when (status) {
            is ExternalUnloadStatus.Submitted -> coinageLogD("Unload submitted group=$group")
            is ExternalUnloadStatus.FinalizedSuccess -> coinageLogI("Unload succeeded group=$group")
            is ExternalUnloadStatus.PartialSuccess ->
                coinageLogW("Unload partially succeeded group=$group executed=${status.executed} total=${status.total} claimed=${status.claimed}")

            is ExternalUnloadStatus.Failed -> coinageLogE("Unload failed group=$group")
        }
    }

    context(balanceContext: CoinageBalanceConversionContext)
    private suspend fun List<CoinageTransactionState>.toUnloadStatus(): ExternalUnloadStatus {
        val executed = filter { it.status == DurableTxStatus.FINALIZED_SUCCESS }

        return when {
            any { it.status.isLive } || isEmpty() -> ExternalUnloadStatus.Submitted

            executed.size == size -> ExternalUnloadStatus.FinalizedSuccess

            executed.isNotEmpty() -> ExternalUnloadStatus.PartialSuccess(
                executed = executed.size,
                total = size,
                claimed = executed.claimedAmount(),
            )

            else -> ExternalUnloadStatus.Failed
        }
    }

    /** Read back from the ledger rather than kept from submission, so it is still known after a restart. */
    context(balanceContext: CoinageBalanceConversionContext)
    private suspend fun List<CoinageTransactionState>.claimedAmount(): Balance {
        val unloaded = flatMap { it.inputs }.filterIsInstance<CoinageInput.Voucher>().map { it.ringVrfIndex }
        val reloaded = flatMap { it.outputs }.filterIsInstance<OwnAsset.Voucher>().map { it.ringVrfIndex }

        return voucherRepository.getByRingVrfKeyIndices(unloaded).totalBalance() -
            voucherRepository.getByRingVrfKeyIndices(reloaded).totalBalance()
    }

    private suspend fun buildGroupExtrinsic(
        chain: Chain,
        prepared: UnloadGroups,
        group: UnloadGroup,
        destination: AccountId,
        personProver: PrecomputedPersonMembershipProver,
    ): Result<EnrichedSendableExtrinsic> {
        val origin = originFactory.createAsUnloadTokenPeopleOrigin(
            recyclerRevisionBlockHash = prepared.pinnedBlockHash,
            vouchers = group.vouchers,
            resolvedUnloadToken = group.resolvedUnloadToken,
            personProver = personProver,
            peopleCollection = prepared.peopleCollection,
        )
        val aliases = buildAliases(group.vouchers)

        return coinageInstanceIdProvider.instanceId().flatMap { instanceId ->
            extrinsicService.buildExtrinsic(
                chain = chain,
                origin = origin,
                options = ExtrinsicService.SubmissionOptions(),
                formExtrinsic = {
                    if (group.mixedOutput != null) {
                        unloadRecyclerIntoExternalAssetAndLoadedCoins(instanceId, group, aliases, destination)
                    } else {
                        unloadRecyclerIntoExternalAsset(instanceId, group, aliases, destination)
                    }
                },
            )
        }
    }

    // --- Supporting helpers ---

    private suspend fun buildAliases(vouchers: List<RecyclerVoucher>): List<BandersnatchAlias> {
        val aliasContext = coinageSigningContextProvider.recyclerVouchersContext()
        return vouchers.map { voucher ->
            voucherRingDerivation.deriveBandersnatch(voucher.ringVrfKeyIndex).aliasInContext(aliasContext)
        }
    }

    private fun List<RecyclerVoucher>.groupByRecycler(): Map<RecyclerKey, List<RecyclerVoucher>> =
        groupBy { RecyclerKey(it.recyclerValue, it.recyclerLocationOrThrow().recyclerIndex) }
}

private fun ExtrinsicBuilder.unloadRecyclerIntoExternalAsset(
    instanceId: CoinageInstanceId,
    group: UnloadGroup,
    aliases: List<BandersnatchAlias>,
    destination: AccountId,
): ExtrinsicBuilder = call(
    moduleName = "Coinage",
    callName = "unload_recycler_into_external_asset",
    arguments = autoEncodedArgs(
        "instance_id" to instanceId.toLong(),
        "aliases" to aliases,
        "value" to group.recyclerKey.exponent,
        "index" to group.recyclerKey.recyclerIndex,
        "revision" to group.revision,
        "to" to destination,
        "max_fee" to Balance.ZERO,
    )
)

private fun ExtrinsicBuilder.unloadRecyclerIntoExternalAssetAndLoadedCoins(
    instanceId: CoinageInstanceId,
    group: UnloadGroup,
    aliases: List<BandersnatchAlias>,
    destination: AccountId,
): ExtrinsicBuilder {
    val mixedOutput = requireNotNull(group.mixedOutput) { "mixedOutput required for and_loaded_coins call" }

    return call(
        moduleName = "Coinage",
        callName = "unload_recycler_into_external_asset_and_loaded_coins",
        arguments = autoEncodedArgs(
            "instance_id" to instanceId.toLong(),
            "aliases" to aliases,
            "value" to group.recyclerKey.exponent,
            "index" to group.recyclerKey.recyclerIndex,
            "revision" to group.revision,
            "to" to destination,
            "external_asset_amount" to mixedOutput.externalAssetAmount,
            "loaded_coins" to mixedOutput.newVouchers.map {
                NewVoucherEntry(value = it.recyclerValue, memberKey = it.ringVrfPublicKey)
            },
            "max_fee" to Balance.ZERO,
        )
    )
}

/**
 * The groups of one unload plus what they all share: every group's revision was read at
 * [pinnedBlockHash], so the person proof they are signed with must be pinned there too.
 */
private data class UnloadGroups(
    val pinnedBlockHash: BlockHash,
    val peopleCollection: PeopleCollection,
    val groups: List<UnloadGroup>,
)

private data class UnloadGroup(
    val recyclerKey: RecyclerKey,
    val vouchers: List<RecyclerVoucher>,
    val resolvedUnloadToken: FreeUnloadTokenResolver.ResolvedUnloadToken,
    val revision: RingRevision,
    val mixedOutput: MixedOutput?,
)

private data class MixedOutput(
    val externalAssetAmount: Balance,
    val newVouchers: List<RecyclerVoucher>,
)

private data class MintedSetup(
    val hostKey: RecyclerKey,
    val newVouchers: List<RecyclerVoucher>,
)

@AsTuple
@Serializable
private data class NewVoucherEntry(
    val value: ValueExponent,
    val memberKey: BandersnatchPublicKey,
)
