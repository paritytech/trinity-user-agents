package io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies.builders

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchAlias
import io.paritytech.polkadotapp.bandersnatch_crypto.aliasInContext
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.repository.ChainStateRepository
import io.paritytech.polkadotapp.chains.util.EncodedArguments.Companion.autoEncodedArgs
import io.paritytech.polkadotapp.chains.util.call
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapIndexedAsync
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.common.utils.progressStallReport.markRegion
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.recyclerLocationOrThrow
import io.paritytech.polkadotapp.feature_coinage_impl.data.config.CoinageInstanceIdProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.VoucherRingDerivation
import io.paritytech.polkadotapp.feature_coinage_impl.data.helpers.FreeUnloadTokenResolver
import io.paritytech.polkadotapp.feature_coinage_impl.data.helpers.UnloadTokenResolverFactory
import io.paritytech.polkadotapp.feature_coinage_impl.data.helpers.createForCollection
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.RecyclerProofDataProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.signer.context.CoinageSigningContextProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.signer.origins.CoinageTransactionOrigins
import io.paritytech.polkadotapp.feature_coinage_impl.domain.model.toSplitDestinations
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.UnloadQuotaTracker
import io.paritytech.polkadotapp.feature_members_api.data.model.RingRevision
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleMembershipProver
import io.paritytech.polkadotapp.feature_people_api.domain.PrecomputedPersonMembershipProver
import io.paritytech.polkadotapp.feature_transactions.api.data.EnrichedSendableExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

/**
 * `Coinage.unload_recycler_into_coins` extrinsics, one per [Unload], built together.
 *
 * Everything one transfer's unloads have in common is resolved once for all of them: the pinned block their
 * ring revisions are read at, the person proof, and the free unload tokens — handed out one per extrinsic,
 * which is also what keeps two extrinsics built together from spending the same token.
 */
class UnloadExtrinsicBuilder @Inject constructor(
    private val chainStateRepository: ChainStateRepository,
    private val originFactory: CoinageTransactionOrigins,
    private val coinageSigningContextProvider: CoinageSigningContextProvider,
    private val voucherRingDerivation: VoucherRingDerivation,
    private val recyclerProofDataProvider: RecyclerProofDataProvider,
    private val unloadTokenResolverFactory: UnloadTokenResolverFactory,
    private val extrinsicService: ExtrinsicService,
    private val peopleMembershipProver: PeopleMembershipProver,
    private val quotaTracker: UnloadQuotaTracker,
    private val coinageInstanceIdProvider: CoinageInstanceIdProvider,
) {
    /** One recycler's [vouchers] unloaded into [outputCoins]. */
    class Unload(
        val vouchers: List<RecyclerVoucher>,
        val outputCoins: List<Coin>,
    ) {
        val recyclerKey: RecyclerKey
            get() = vouchers.first().let { RecyclerKey(it.recyclerValue, it.recyclerLocationOrThrow().recyclerIndex) }
    }

    /** In the order of [unloads]. */
    context(diagnostics: StalenessReportCollector)
    suspend fun build(
        unloads: List<Unload>,
        peopleCollection: PeopleCollection,
        chain: Chain,
    ): Result<List<EnrichedSendableExtrinsic>> = runCatching {
        val context = resolveUnloadContext(unloads, chain)
        val freeUnloadTokens = resolveFreeUnloadTokens(unloads.size, peopleCollection, chain)
        val personProver = precomputePersonProver(context.pinnedBlockHash, peopleCollection, chain)

        buildAll(unloads, context, freeUnloadTokens, personProver, peopleCollection, chain)
    }

    /**
     * After a successful build, not after resolving: a token picked for an extrinsic that was never built is still
     * there to be picked again. An attempt the engine then declines to start is over-counted until the tracker's
     * next refresh, which is the safe direction.
     */
    suspend fun noteUnloadsHappened(count: Int) {
        quotaTracker.noteUnloadsHappened(count)
    }

    context(diagnostics: StalenessReportCollector)
    private suspend fun resolveUnloadContext(
        unloads: List<Unload>,
        chain: Chain,
    ): UnloadContext = diagnostics.markRegion(RCommon.string.stall_reading_chain_state) {
        val pinnedBlockHash = chainStateRepository.currentBlockHash(chain.id)
        val groupRevisions = recyclerProofDataProvider
            .getRecyclerRevisions(chain.id, unloads.map { it.recyclerKey }.distinct(), pinnedBlockHash)
            .logFailure("Failed to get recycler revisions")
            .getOrThrow()

        UnloadContext(pinnedBlockHash, groupRevisions)
    }

    context(diagnostics: StalenessReportCollector)
    private suspend fun resolveFreeUnloadTokens(
        count: Int,
        peopleCollection: PeopleCollection,
        chain: Chain,
    ): List<FreeUnloadTokenResolver.ResolvedUnloadToken> =
        diagnostics.markRegion(RCommon.string.coinage_stall_picking_unload_token) {
            unloadTokenResolverFactory.createForCollection(peopleCollection).resolve(chain.id, count)
        }

    /**
     * One prover for the whole transfer: every unload proves the same person against the same pinned block, so
     * the ring lookups behind it are paid once instead of once per extrinsic.
     */
    context(diagnostics: StalenessReportCollector)
    private suspend fun precomputePersonProver(
        pinnedBlockHash: BlockHash,
        peopleCollection: PeopleCollection,
        chain: Chain,
    ): PrecomputedPersonMembershipProver = diagnostics.markRegion(RCommon.string.coinage_stall_reading_anonymity_set) {
        peopleMembershipProver.precomputeForMember(
            chainId = chain.id,
            peopleCollection = peopleCollection,
            at = pinnedBlockHash,
        ).getOrThrow()
    }

    /**
     * The ring-VRF proofs each extrinsic carries are produced lazily while it is being built, and they are the
     * slowest thing in the transfer. The unloads run concurrently, so one region covers the whole fan-out
     * rather than one row per extrinsic.
     */
    context(diagnostics: StalenessReportCollector)
    private suspend fun buildAll(
        unloads: List<Unload>,
        context: UnloadContext,
        freeUnloadTokens: List<FreeUnloadTokenResolver.ResolvedUnloadToken>,
        personProver: PrecomputedPersonMembershipProver,
        peopleCollection: PeopleCollection,
        chain: Chain,
    ): List<EnrichedSendableExtrinsic> = diagnostics.markRegion(RCommon.string.coinage_stall_generating_proofs) {
        unloads.mapIndexedAsync { index, unload ->
            buildExtrinsic(
                unload = unload,
                unloadToken = freeUnloadTokens[index],
                personProver = personProver,
                recyclerRevisionBlockHash = context.pinnedBlockHash,
                revision = context.groupRevisions.getValue(unload.recyclerKey),
                peopleCollection = peopleCollection,
                chain = chain,
            ).getOrThrow()
        }
    }

    private suspend fun buildExtrinsic(
        unload: Unload,
        unloadToken: FreeUnloadTokenResolver.ResolvedUnloadToken,
        personProver: PrecomputedPersonMembershipProver,
        recyclerRevisionBlockHash: BlockHash,
        revision: RingRevision,
        peopleCollection: PeopleCollection,
        chain: Chain,
    ) = coinageInstanceIdProvider.instanceId().flatMap { instanceId ->
        val destinations = unload.outputCoins.toSplitDestinations()
        val origin = makeOriginDefinition(unload.vouchers, unloadToken, recyclerRevisionBlockHash, personProver, peopleCollection)
        val aliases = buildAliases(unload.vouchers)

        extrinsicService.buildExtrinsic(
            chain = chain,
            origin = origin,
            options = ExtrinsicService.SubmissionOptions(),
            formExtrinsic = {
                call(
                    moduleName = "Coinage",
                    callName = "unload_recycler_into_coins",
                    arguments = autoEncodedArgs(
                        "instance_id" to instanceId.toLong(),
                        "aliases" to aliases,
                        "value" to unload.recyclerKey.exponent,
                        "index" to unload.recyclerKey.recyclerIndex,
                        "revision" to revision,
                        "split_into" to destinations,
                        "max_fee" to Balance.ZERO,
                    ),
                )
            },
        )
    }

    private suspend fun buildAliases(vouchers: List<RecyclerVoucher>): List<BandersnatchAlias> {
        val aliasContext = coinageSigningContextProvider.recyclerVouchersContext()

        return vouchers.map { voucher ->
            voucherRingDerivation.deriveBandersnatch(voucher.ringVrfKeyIndex)
                .aliasInContext(aliasContext)
        }
    }

    private fun makeOriginDefinition(
        vouchers: List<RecyclerVoucher>,
        resolvedUnloadToken: FreeUnloadTokenResolver.ResolvedUnloadToken,
        recyclerRevisionBlockHash: BlockHash,
        personProver: PrecomputedPersonMembershipProver,
        peopleCollection: PeopleCollection,
    ): TransactionOrigin {
        return originFactory.createAsUnloadTokenPeopleOrigin(
            vouchers = vouchers,
            resolvedUnloadToken = resolvedUnloadToken,
            recyclerRevisionBlockHash = recyclerRevisionBlockHash,
            personProver = personProver,
            peopleCollection = peopleCollection,
        )
    }

    private class UnloadContext(
        val pinnedBlockHash: BlockHash,
        val groupRevisions: Map<RecyclerKey, RingRevision>,
    )
}
