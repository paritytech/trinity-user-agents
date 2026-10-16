package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.harness

import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageAssetState
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageAssetStates
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageInput
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionState
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.TEST_INSTALLATION
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.AssetPublicKey
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.AssetRegistration
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.CoinageAssetKind
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.CoinageAssetLedger
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.EntryAssets
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.LedgerAsset
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.LedgerEntry
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.COINAGE_DOMAIN
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.CheckpointBlock
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxEntry
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxState
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.OperationGroupId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.RegistrationScope
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.ScheduledDurableTx
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPolicy
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPolicyId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TxDomainId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.Verdict
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionHash
import io.paritytech.polkadotapp.feature_transactions_impl.data.durable.DurableTxAttempt
import io.paritytech.polkadotapp.feature_transactions_impl.data.durable.DurableTxRegistration
import io.paritytech.polkadotapp.feature_transactions_impl.data.durable.DurableTxRepository
import io.paritytech.polkadotapp.feature_transactions_impl.data.durable.DurableTxSchedule
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

internal data class HandoffRow(val asset: OwnAsset, val committed: Boolean)

/**
 * The ledger, in memory, standing in for Room so a crash can be modelled as dropping everything volatile
 * while this store survives.
 *
 * One store rather than two, because in production there is one database: the engine's transaction row and
 * coinage's asset rows commit together or not at all, and a snapshot taken here restores both — which is the
 * only way a caller can observe SQLite's rollback.
 *
 * Asset state is derived on read, exactly as `ASSET_STATE_QUERY` derives it: the key space is the union of
 * outputs, own inputs and handoff marks, and each of the three facts is a separate lookup over that space.
 */
class InMemoryLedger {
    internal val mutex = Mutex()
    internal val revisions = MutableStateFlow(0)

    internal var rows: List<DurableTxEntry> = emptyList()

    /** Rows scheduled and never built. They sit in [rows] with a placeholder attempt nothing may read. */
    internal var unbuilt: Set<DurableTxId> = emptySet()
    internal var policies: Map<DurableTxId, SubmissionPolicy> = emptyMap()
    internal var assets: Map<CoinageTransactionId, EntryAssets> = emptyMap()
    internal var handoffs: Map<AssetPublicKey, HandoffRow> = emptyMap()
    internal var nextId = 1L

    val engine: DurableTxRepository = InMemoryDurableTxRepository(this)
    val coinage: CoinageAssetLedger = InMemoryCoinageAssetLedger(this)

    /**
     * The reads scenarios and the fuzzer make of the ledger, under the shape they used when the transaction
     * row and its assets were one table. Kept so their bodies did not have to learn about the split.
     */
    val repository = HarnessLedgerReads(this)

    /** Entries as the rules see them: the engine's row joined to coinage's assets. */
    fun entries(): List<LedgerEntry> = rows.sortedBy { it.id.value }.map { it.toEntry() }

    fun entryOrNull(id: CoinageTransactionId): LedgerEntry? =
        rows.firstOrNull { it.id == id }?.toEntry()

    private fun DurableTxEntry.toEntry(): LedgerEntry {
        val entryAssets = assets[id] ?: EntryAssets(emptyList(), emptyList())

        return LedgerEntry(this, entryAssets.inputs, entryAssets.outputs)
    }

    internal suspend fun <T> transaction(block: suspend () -> T): Result<T> = mutex.withLock {
        val rowsBefore = rows
        val unbuiltBefore = unbuilt
        val policiesBefore = policies
        val assetsBefore = assets
        val handoffsBefore = handoffs
        val nextIdBefore = nextId

        runCatching { block() }
            .onSuccess { revisions.value++ }
            .onFailure {
                rows = rowsBefore
                unbuilt = unbuiltBefore
                policies = policiesBefore
                assets = assetsBefore
                handoffs = handoffsBefore
                nextId = nextIdBefore
            }
    }

    internal suspend fun <T> read(block: () -> T): Result<T> = mutex.withLock { runCatching(block) }

    internal fun assetStates(): Map<OwnAsset, CoinageAssetState> {
        val entries = entries()

        val keySpace = buildSet {
            entries.forEach { entry ->
                entry.outputs.forEach { it.asset?.let(::add) }
                entry.inputs.forEach { it.asset?.let(::add) }
            }
            handoffs.values.forEach { add(it.asset) }
        }

        return keySpace.associateWith { asset ->
            CoinageAssetState(
                handedOff = handoffs.values.any { it.asset == asset },
                minterStatus = entries.firstOrNull { entry -> entry.outputs.any { it.asset == asset } }?.status,
                consumerStatus = entries
                    .firstOrNull { entry ->
                        entry.status != DurableTxStatus.FAILURE && entry.inputs.any { it.asset == asset }
                    }
                    ?.status,
            )
        }
    }
}

class HarnessLedgerReads(private val store: InMemoryLedger) {
    suspend fun getEntry(id: CoinageTransactionId): Result<LedgerEntry?> =
        store.read { store.entryOrNull(id) }

    suspend fun getAllEntries(): Result<List<LedgerEntry>> = store.read { store.entries() }

    suspend fun hasLiveEntries(): Result<Boolean> = store.read { store.rows.any { it.status.isLive } }

    suspend fun getStatus(id: CoinageTransactionId): Result<DurableTxStatus?> =
        store.read { store.rows.firstOrNull { it.id == id }?.status }

    suspend fun getHandoffKeys(): Result<Set<AssetPublicKey>> = store.coinage.getHandoffKeys()

    suspend fun getAssetState(asset: OwnAsset): Result<CoinageAssetState> = store.coinage.getAssetState(asset)
}

private class InMemoryDurableTxRepository(private val store: InMemoryLedger) : DurableTxRepository {
    override suspend fun register(
        registration: DurableTxRegistration,
        onRegister: suspend RegistrationScope.(DurableTxId) -> Unit,
    ): Result<DurableTxId> = store.transaction {
        val id = store.insert(registration)
        RegistrationScope.onRegister(id)

        id
    }

    override suspend fun registerAll(
        registrations: List<DurableTxRegistration>,
        onRegister: suspend RegistrationScope.(List<DurableTxId>) -> Unit,
    ): Result<List<DurableTxId>> = store.transaction {
        val ids = registrations.map { store.insert(it) }
        RegistrationScope.onRegister(ids)

        ids
    }

    override suspend fun schedule(
        schedules: List<DurableTxSchedule>,
        onRegister: suspend RegistrationScope.(List<DurableTxId>) -> Unit,
    ): Result<List<DurableTxId>> = store.transaction {
        val ids = schedules.map { store.insertScheduled(it) }
        RegistrationScope.onRegister(ids)

        ids
    }

    override suspend fun getEntry(id: DurableTxId): Result<DurableTxEntry?> =
        store.read { store.rows.firstOrNull { it.id == id && it.id !in store.unbuilt } }

    override suspend fun getAllEntries(domainId: TxDomainId): Result<List<DurableTxEntry>> = store.read {
        store.rows
            .filter { it.domainId == domainId && it.status != DurableTxStatus.PENDING_SUBMISSION }
            .sortedBy { it.id.value }
    }

    override suspend fun getSubmissionPolicy(id: DurableTxId): Result<SubmissionPolicy?> =
        store.read { store.policies[id] }

    override fun subscribePendingSubmissions(): Flow<List<ScheduledDurableTx>> =
        store.revisions.map { store.pendingSubmissions { true } }

    override suspend fun getPendingSubmissions(
        policyId: SubmissionPolicyId,
        groupId: OperationGroupId?,
    ): Result<List<ScheduledDurableTx>> = store.read {
        store.pendingSubmissions { it.policy.id == policyId && it.groupId == groupId }
    }

    override suspend fun startAttempt(id: DurableTxId, attempt: DurableTxAttempt): Result<Boolean> = store.transaction {
        val current = store.rows.firstOrNull { it.id == id }

        if (current?.status != DurableTxStatus.PENDING_SUBMISSION) {
            false
        } else {
            store.rows = store.rows.map {
                if (it.id == id) {
                    it.copy(
                        txHash = attempt.txHash,
                        checkpoint = attempt.checkpoint,
                        mortalityBlocks = attempt.mortalityBlocks,
                        status = DurableTxStatus.PENDING,
                        successDetectedAt = null,
                    )
                } else {
                    it
                }
            }
            store.unbuilt = store.unbuilt - id
            true
        }
    }

    override suspend fun abandonSubmission(id: DurableTxId): Result<Boolean> = store.transaction {
        val current = store.rows.firstOrNull { it.id == id }

        if (current?.status != DurableTxStatus.PENDING_SUBMISSION) {
            false
        } else {
            store.rows = store.rows.map { if (it.id == id) it.copy(status = DurableTxStatus.FAILURE) else it }
            true
        }
    }

    override suspend fun getStatus(id: DurableTxId): Result<DurableTxStatus?> =
        store.read { store.rows.firstOrNull { it.id == id }?.status }

    override fun subscribeStatus(id: DurableTxId): Flow<DurableTxStatus> =
        store.revisions.map { store.rows.firstOrNull { it.id == id }?.status }.filterNotNull()

    override suspend fun compareAndSetStatus(
        id: DurableTxId,
        observed: DurableTxStatus,
        observedTxHash: TransactionHash,
        verdict: Verdict,
    ): Result<Boolean> = store.transaction {
        val current = store.rows.firstOrNull { it.id == id }

        if (current == null || current.status != observed || current.txHash != observedTxHash) {
            false
        } else {
            store.rows = store.rows.map {
                if (it.id == id) {
                    it.copy(status = verdict.status, successDetectedAt = verdict.successDetectedAt)
                } else {
                    it
                }
            }
            true
        }
    }

    override suspend fun hasLiveTransactions(): Result<Boolean> =
        store.read { store.rows.any { it.status.isLive } }

    override suspend fun liveDomains(): Result<List<TxDomainId>> =
        store.read { store.rows.filter { it.status.awaitsVerdict }.map { it.domainId }.distinct() }

    override suspend fun getGroupStates(
        domainId: TxDomainId,
        groupId: OperationGroupId,
    ): Result<List<DurableTxState>> = store.read { store.groupRows(domainId, groupId).map { it.toState() } }

    override fun subscribeGroupStates(
        domainId: TxDomainId,
        groupId: OperationGroupId,
    ): Flow<List<DurableTxState>> =
        store.revisions.map { store.groupRows(domainId, groupId).map { it.toState() } }

    private fun DurableTxEntry.toState() = DurableTxState(id, status)
}

private class InMemoryCoinageAssetLedger(private val store: InMemoryLedger) : CoinageAssetLedger {
    /**
     * Called from inside the engine's transaction, which already holds the lock and will restore the
     * snapshot if this throws — so it neither locks nor rolls back itself.
     */
    context(_: RegistrationScope)
    override suspend fun registerAssets(registrations: List<Pair<CoinageTransactionId, AssetRegistration>>) {
        brokenInvariant(registrations.map { it.second })?.let { throw it }

        registrations.forEach { (id, registration) ->
            store.assets = store.assets + (
                id to EntryAssets(
                    inputs = registration.inputs.map {
                        LedgerAsset(it.input.kind(), it.input.ownOrNull(), it.publicKey)
                    },
                    outputs = registration.outputs.map {
                        LedgerAsset(it.output.kind(), it.output, it.publicKey)
                    },
                )
                )
        }
    }

    private fun brokenInvariant(registrations: List<AssetRegistration>): Throwable? {
        if (registrations.any { it.inputs.isEmpty() && it.outputs.isEmpty() }) {
            return io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model
                .CoinageRegistrationError.EmptyTransaction
        }

        val outputs = registrations.flatMap { it.outputs }
        val inputs = registrations.flatMap { it.inputs }

        val outputKeys = outputs.map { it.publicKey }
        val inputKeys = inputs.map { it.publicKey }

        val notFresh = store.filterMinted(outputKeys) + store.filterReceived(outputKeys) + outputKeys.duplicates()
        outputs.firstOrNull { it.publicKey in notFresh }?.let {
            return io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model
                .CoinageRegistrationError.OutputNotFresh(it.output)
        }

        val handedOff = inputKeys.filterTo(mutableSetOf()) { it in store.handoffs }
        inputs.firstOrNull { it.publicKey in handedOff }?.let {
            return io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model
                .CoinageRegistrationError.InputHandedOff(it.input)
        }

        val claimed = store.filterClaimed(inputKeys) + inputKeys.duplicates()
        inputs.firstOrNull { it.publicKey in claimed }?.let {
            return io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model
                .CoinageRegistrationError.InputAlreadyClaimed(it.input)
        }

        return null
    }

    override suspend fun markHandedOff(assets: List<LedgerAsset>): Result<Unit> = store.transaction {
        val claimed = store.filterClaimed(assets.map { it.publicKey })
        assets.firstOrNull { it.publicKey in claimed }?.asset?.let {
            throw io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model
                .CoinageRegistrationError.HandoffOfClaimedAsset(it)
        }

        assets.firstOrNull { it.publicKey in store.handoffs }?.asset?.let {
            throw io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model
                .CoinageRegistrationError.HandoffOfHandedOffAsset(it)
        }

        assets.forEach { asset ->
            val owned = asset.asset ?: return@forEach
            if (asset.publicKey !in store.handoffs) {
                store.handoffs = store.handoffs + (asset.publicKey to HandoffRow(owned, committed = false))
            }
        }
    }

    override suspend fun commitHandoffs(keys: List<AssetPublicKey>): Result<Unit> = store.transaction {
        store.handoffs = store.handoffs.mapValues { (key, row) ->
            if (key in keys) row.copy(committed = true) else row
        }
    }

    override suspend fun releaseUncommittedHandoffs(): Result<Unit> = store.transaction {
        store.handoffs = store.handoffs.filterValues { it.committed }
    }

    override suspend fun releaseUncommittedHandoffs(keys: List<AssetPublicKey>): Result<Unit> = store.transaction {
        store.handoffs = store.handoffs.filter { (key, row) -> row.committed || key !in keys }
    }

    override suspend fun getHandoffKeys(): Result<Set<AssetPublicKey>> = store.read { store.handoffs.keys.toSet() }

    override suspend fun assetsOf(
        ids: List<CoinageTransactionId>,
    ): Result<Map<CoinageTransactionId, EntryAssets>> = store.read {
        ids.associateWith { store.assets[it] ?: EntryAssets(emptyList(), emptyList()) }
    }

    override suspend fun getGroupStatuses(
        groupId: CoinageOperationGroupId,
    ): Result<List<CoinageTransactionState>> = store.read { store.groupStates(groupId) }

    override fun subscribeGroupStatuses(
        groupId: CoinageOperationGroupId,
    ): Flow<List<CoinageTransactionState>> = store.revisions.map { store.groupStates(groupId) }

    override fun subscribeAssetStates(): Flow<CoinageAssetStates> =
        store.revisions.map { CoinageAssetStates(store.assetStates(), TEST_INSTALLATION) }

    override suspend fun getAssetState(asset: OwnAsset): Result<CoinageAssetState> =
        store.read { CoinageAssetStates(store.assetStates(), TEST_INSTALLATION).getAssetStateOf(asset) }

    override suspend fun getAssetStates(assets: List<OwnAsset>): Result<CoinageAssetStates> =
        store.read { CoinageAssetStates(store.assetStates(), TEST_INSTALLATION) }
}

private fun InMemoryLedger.insert(registration: DurableTxRegistration): DurableTxId {
    val id = DurableTxId(nextId++)

    rows = rows + DurableTxEntry(
        id = id,
        domainId = registration.domainId,
        groupId = registration.groupId,
        txHash = registration.attempt.txHash,
        checkpoint = registration.attempt.checkpoint,
        mortalityBlocks = registration.attempt.mortalityBlocks,
        status = DurableTxStatus.PENDING,
        successDetectedAt = null,
    )
    registration.policy?.let { policies = policies + (id to it) }

    return id
}

private fun InMemoryLedger.insertScheduled(schedule: DurableTxSchedule): DurableTxId {
    val id = DurableTxId(nextId++)

    rows = rows + DurableTxEntry(
        id = id,
        domainId = schedule.domainId,
        groupId = schedule.groupId,
        txHash = "",
        checkpoint = CheckpointBlock(blockNumber = 0, blockHash = ""),
        mortalityBlocks = 0,
        status = DurableTxStatus.PENDING_SUBMISSION,
        successDetectedAt = null,
    )
    unbuilt = unbuilt + id
    policies = policies + (id to schedule.policy)

    return id
}

private fun InMemoryLedger.pendingSubmissions(filter: (ScheduledDurableTx) -> Boolean): List<ScheduledDurableTx> =
    rows.filter { it.status == DurableTxStatus.PENDING_SUBMISSION }
        .sortedBy { it.id.value }
        .mapNotNull { row ->
            policies[row.id]?.let { ScheduledDurableTx(row.id, row.domainId, row.groupId, it) }
        }
        .filter(filter)

private fun InMemoryLedger.groupRows(domainId: TxDomainId, groupId: OperationGroupId) =
    rows.filter { it.domainId == domainId && it.groupId == groupId }.sortedBy { it.id.value }

private fun InMemoryLedger.groupStates(groupId: CoinageOperationGroupId) =
    groupRows(COINAGE_DOMAIN, groupId).map { entry ->
        val entryAssets = assets[entry.id] ?: EntryAssets(emptyList(), emptyList())

        CoinageTransactionState(
            id = entry.id,
            status = entry.status,
            inputs = entryAssets.inputs.map { it.toCoinageInput() },
            outputs = entryAssets.outputs.mapNotNull { it.asset },
        )
    }

private fun InMemoryLedger.filterMinted(keys: List<AssetPublicKey>) =
    keys.filterTo(mutableSetOf()) { key -> assets.values.any { it.outputs.any { o -> o.publicKey == key } } }

private fun InMemoryLedger.filterReceived(keys: List<AssetPublicKey>) =
    keys.filterTo(mutableSetOf()) { key ->
        assets.values.any { it.inputs.any { i -> i.publicKey == key && i.asset == null } }
    }

private fun InMemoryLedger.filterClaimed(keys: List<AssetPublicKey>) =
    keys.filterTo(mutableSetOf()) { key ->
        entries().any { entry ->
            entry.status != DurableTxStatus.FAILURE && entry.inputs.any { it.publicKey == key }
        }
    }

private fun List<AssetPublicKey>.duplicates(): Set<AssetPublicKey> =
    groupingBy { it }.eachCount().filterValues { it > 1 }.keys

/** The mirror of [CoinageInput.kind]/[CoinageInput.ownOrNull]: a null asset is a coin a peer sent us. */
private fun LedgerAsset.toCoinageInput(): CoinageInput = when (val ownAsset = asset) {
    is OwnAsset.Coin -> CoinageInput.Coin.Own(ownAsset.derivationIndex)
    is OwnAsset.Voucher -> CoinageInput.Voucher(ownAsset.ringVrfIndex)
    null -> CoinageInput.Coin.Received(publicKey)
}

private fun CoinageInput.kind() = when (this) {
    is CoinageInput.Coin -> CoinageAssetKind.COIN
    is CoinageInput.Voucher -> CoinageAssetKind.VOUCHER
}

private fun CoinageInput.ownOrNull(): OwnAsset? = when (this) {
    is CoinageInput.Coin.Own -> OwnAsset.Coin(derivationIndex)
    is CoinageInput.Coin.Received -> null
    is CoinageInput.Voucher -> OwnAsset.Voucher(ringVrfIndex)
}

private fun OwnAsset.kind() = when (this) {
    is OwnAsset.Coin -> CoinageAssetKind.COIN
    is OwnAsset.Voucher -> CoinageAssetKind.VOUCHER
}
