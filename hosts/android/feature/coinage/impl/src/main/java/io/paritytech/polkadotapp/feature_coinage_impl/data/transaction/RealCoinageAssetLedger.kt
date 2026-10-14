package io.paritytech.polkadotapp.feature_coinage_impl.data.transaction

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.database.dao.CoinageAssetStateProjection
import io.paritytech.polkadotapp.database.dao.CoinageEntryDao
import io.paritytech.polkadotapp.database.dao.CoinageEntryWithAssets
import io.paritytech.polkadotapp.database.model.CoinageAssetKindLocal
import io.paritytech.polkadotapp.database.model.CoinageEntryInputLocal
import io.paritytech.polkadotapp.database.model.CoinageEntryOutputLocal
import io.paritytech.polkadotapp.database.model.CoinageHandoffLocal
import io.paritytech.polkadotapp.database.model.DurableTxLocal
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageAssetState
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageAssetStates
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageInput
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageRegistrationError
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionState
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.data.installation.CoinageInstallationRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.installation.queryPerInstallation
import io.paritytech.polkadotapp.feature_coinage_impl.data.installation.toCoinageKeyIndex
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.RegistrationScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

class RealCoinageAssetLedger @Inject constructor(
    private val dao: CoinageEntryDao,
    private val installationRepository: CoinageInstallationRepository,
) : CoinageAssetLedger {
    context(_: RegistrationScope)
    override suspend fun registerAssets(registrations: List<Pair<CoinageTransactionId, AssetRegistration>>) {
        brokenInvariant(registrations.map { it.second })?.let { throw it }

        registrations.forEach { (id, registration) ->
            dao.insertInputs(registration.inputs.mapIndexed { position, it -> it.toLocal(id.value, position) })
            dao.insertOutputs(registration.outputs.mapIndexed { position, it -> it.toLocal(id.value, position) })
        }
    }

    /**
     * The invariants, checked across the whole batch: these rows do not exist yet, so the queries cannot
     * see one registration's assets while checking another's.
     */
    private suspend fun brokenInvariant(registrations: List<AssetRegistration>): CoinageRegistrationError? {
        val scope = DaoValidationScope(dao)

        // Nothing to lock and nothing to look for on chain, so the rules could never decide it.
        if (registrations.any { it.inputs.isEmpty() && it.outputs.isEmpty() }) {
            return CoinageRegistrationError.EmptyTransaction
        }

        val outputs = registrations.flatMap { it.outputs }
        val inputs = registrations.flatMap { it.inputs }

        val outputKeys = outputs.map { it.publicKey }
        val inputKeys = inputs.map { it.publicKey }

        // Fresh outputs: an output address is an output of no other transaction, and not a key a peer sent us.
        val notFresh = scope.filterMinted(outputKeys) + scope.filterReceived(outputKeys) + outputKeys.duplicates()
        outputs.firstOrNull { it.publicKey in notFresh }?.let {
            return CoinageRegistrationError.OutputNotFresh(it.output)
        }

        // Blocked handoff: a handed-off asset is never an input.
        val handedOff = scope.filterHandedOff(inputKeys)
        inputs.firstOrNull { it.publicKey in handedOff }?.let {
            return CoinageRegistrationError.InputHandedOff(it.input)
        }

        // Unique consumer: an asset is an input of at most one transaction that is not a failure.
        val claimed = scope.filterClaimed(inputKeys) + inputKeys.duplicates()
        inputs.firstOrNull { it.publicKey in claimed }?.let {
            return CoinageRegistrationError.InputAlreadyClaimed(it.input)
        }

        return null
    }

    override suspend fun markHandedOff(assets: List<LedgerAsset>): Result<Unit> = runCatching {
        val keys = assets.map { it.publicKey }

        // One transaction, so nothing can claim these between the check and the mark. Unlike registration,
        // no engine transaction is open around this call: a handoff writes coinage rows only.
        dao.withTransaction {
            val scope = DaoValidationScope(dao)

            // The mirror of Blocked handoff: an asset a transaction of ours still has a claim on cannot
            // also leave the device, or the peer and that transaction would both be spending it.
            val claimed = scope.filterClaimed(keys)
            assets.firstOrNull { it.publicKey in claimed }?.asset?.let {
                throw CoinageRegistrationError.HandoffOfClaimedAsset(it)
            }

            // Single handoff: a key that already left the device cannot leave it again, or two peers would
            // hold the same private key. The insert alone would not say so — it ignores the conflict.
            val handedOff = scope.filterHandedOff(keys)
            assets.firstOrNull { it.publicKey in handedOff }?.asset?.let {
                throw CoinageRegistrationError.HandoffOfHandedOffAsset(it)
            }

            dao.insertHandoffs(assets.mapNotNull { it.toHandoffLocal() })
        }
    }

    override suspend fun commitHandoffs(keys: List<AssetPublicKey>): Result<Unit> = runCatching {
        dao.commitHandoffs(keys.map { it.value })
    }

    override suspend fun releaseUncommittedHandoffs(): Result<Unit> = runCatching {
        dao.deleteUncommittedHandoffs()
    }

    override suspend fun releaseUncommittedHandoffs(keys: List<AssetPublicKey>): Result<Unit> = runCatching {
        dao.deleteUncommittedHandoffs(keys.map { it.value })
    }

    override suspend fun getHandoffKeys(): Result<Set<AssetPublicKey>> = runCatching {
        dao.getHandoffs().mapTo(mutableSetOf()) { it.onChainKey.toDataByteArray() }
    }

    override suspend fun assetsOf(
        ids: List<CoinageTransactionId>,
    ): Result<Map<CoinageTransactionId, EntryAssets>> = runCatching {
        if (ids.isEmpty()) return@runCatching emptyMap()

        val raw = ids.map { it.value }
        val inputs = dao.inputsOf(raw).groupBy { it.entryId }
        val outputs = dao.outputsOf(raw).groupBy { it.entryId }

        ids.associateWith { id ->
            EntryAssets(
                inputs = inputs[id.value].orEmpty().sortedBy { it.position }.map { it.toDomain() },
                outputs = outputs[id.value].orEmpty().sortedBy { it.position }.map { it.toDomain() },
            )
        }
    }

    override suspend fun getGroupStatuses(
        groupId: CoinageOperationGroupId,
    ): Result<List<CoinageTransactionState>> = runCatching {
        dao.getGroupEntries(groupId.value).toTransactionStates()
    }

    override fun subscribeGroupStatuses(
        groupId: CoinageOperationGroupId,
    ): Flow<List<CoinageTransactionState>> =
        dao.subscribeGroupEntries(groupId.value).map { it.toTransactionStates() }

    override fun subscribeAssetStates(): Flow<CoinageAssetStates> =
        dao.subscribeAssetStates().map { it.toAssetStates() }

    override suspend fun getAssetState(asset: OwnAsset): Result<CoinageAssetState> = runCatching {
        val index = asset.index()
        val projection = dao.getAssetState(asset.kind().toLocal(), index.installation.value.value, index.item)

        listOfNotNull(projection).toAssetStates().getAssetStateOf(asset)
    }

    override suspend fun getAssetStates(assets: List<OwnAsset>): Result<CoinageAssetStates> = runCatching {
        assets.groupBy { it.kind() }
            .flatMap { (kind, ofKind) ->
                ofKind.map { it.index() }.queryPerInstallation { installationId, items ->
                    dao.getAssetStates(kind.toLocal(), installationId, items)
                }
            }
            .toAssetStates()
    }

    private suspend fun List<CoinageAssetStateProjection>.toAssetStates() =
        CoinageAssetStates(associate { it.toDomain() }, installationRepository.getOrCreateCurrent())
}

private class DaoValidationScope(private val dao: CoinageEntryDao) : RegistrationValidationScope {
    override suspend fun filterMinted(keys: List<AssetPublicKey>) = dao.filterMinted(keys.raw()).wrap()

    override suspend fun filterReceived(keys: List<AssetPublicKey>) = dao.filterReceivedKeys(keys.raw()).wrap()

    override suspend fun filterClaimed(keys: List<AssetPublicKey>) = dao.filterClaimed(keys.raw()).wrap()

    override suspend fun filterHandedOff(keys: List<AssetPublicKey>) = dao.filterHandedOff(keys.raw()).wrap()

    private fun List<AssetPublicKey>.raw() = map { it.value }

    private fun List<ByteArray>.wrap() = mapTo(mutableSetOf()) { it.toDataByteArray() }
}

private fun List<AssetPublicKey>.duplicates(): Set<AssetPublicKey> =
    groupingBy { it }.eachCount().filterValues { it > 1 }.keys

private fun RegistrationInput.toLocal(entryId: Long, position: Int) = CoinageEntryInputLocal(
    entryId = entryId,
    position = position,
    assetKind = input.kind().toLocal(),
    installationId = input.indexOrNull()?.installation?.value?.value,
    derivationIndex = input.indexOrNull()?.item,
    onChainKey = publicKey.value,
)

private fun RegistrationOutput.toLocal(entryId: Long, position: Int) = CoinageEntryOutputLocal(
    entryId = entryId,
    position = position,
    assetKind = output.kind().toLocal(),
    installationId = output.index().installation.value.value,
    derivationIndex = output.index().item,
    onChainKey = publicKey.value,
)

private fun LedgerAsset.toHandoffLocal(): CoinageHandoffLocal? {
    val owned = asset ?: return null

    return CoinageHandoffLocal(
        onChainKey = publicKey.value,
        assetKind = owned.kind().toLocal(),
        installationId = owned.index().installation.value.value,
        derivationIndex = owned.index().item,
        committed = false,
    )
}

private fun CoinageEntryInputLocal.toDomain() = LedgerAsset(
    kind = assetKind.toDomain(),
    asset = derivationIndex?.let { item -> assetKind.toOwnAsset(requireNotNull(installationId).toCoinageKeyIndex(item)) },
    publicKey = onChainKey.toDataByteArray(),
)

private fun CoinageEntryOutputLocal.toDomain() = LedgerAsset(
    kind = assetKind.toDomain(),
    asset = assetKind.toOwnAsset(installationId.toCoinageKeyIndex(derivationIndex)),
    publicKey = onChainKey.toDataByteArray(),
)

private fun List<CoinageEntryWithAssets>.toTransactionStates() = map { row ->
    CoinageTransactionState(
        id = CoinageTransactionId(row.entry.id),
        status = row.entry.status.toDomain(),
        inputs = row.inputs.sortedBy { it.position }.map { it.toDomain().toCoinageInput() },
        outputs = row.outputs.sortedBy { it.position }.mapNotNull { it.toDomain().asset },
    )
}

/** A null [LedgerAsset.asset] is the peer-sent coin case: an on-chain identity we hold no local asset for. */
private fun LedgerAsset.toCoinageInput(): CoinageInput = when (val ownAsset = asset) {
    is OwnAsset.Coin -> CoinageInput.Coin.Own(ownAsset.derivationIndex)
    is OwnAsset.Voucher -> CoinageInput.Voucher(ownAsset.ringVrfIndex)
    null -> CoinageInput.Coin.Received(publicKey)
}

private fun CoinageAssetStateProjection.toDomain(): Pair<OwnAsset, CoinageAssetState> =
    assetKind.toOwnAsset(installationId.toCoinageKeyIndex(derivationIndex)) to CoinageAssetState(
        handedOff = handedOff,
        minterStatus = minterStatus?.toDomain(),
        consumerStatus = consumerStatus?.toDomain(),
    )

private fun CoinageAssetKindLocal.toOwnAsset(derivationIndex: CoinageKeyIndex): OwnAsset = when (this) {
    CoinageAssetKindLocal.COIN -> OwnAsset.Coin(derivationIndex)
    CoinageAssetKindLocal.VOUCHER -> OwnAsset.Voucher(derivationIndex)
}

private fun CoinageAssetKindLocal.toDomain() = when (this) {
    CoinageAssetKindLocal.COIN -> CoinageAssetKind.COIN
    CoinageAssetKindLocal.VOUCHER -> CoinageAssetKind.VOUCHER
}

private fun CoinageAssetKind.toLocal() = when (this) {
    CoinageAssetKind.COIN -> CoinageAssetKindLocal.COIN
    CoinageAssetKind.VOUCHER -> CoinageAssetKindLocal.VOUCHER
}

private fun DurableTxLocal.Status.toDomain() = when (this) {
    DurableTxLocal.Status.PENDING -> DurableTxStatus.PENDING
    DurableTxLocal.Status.PENDING_SUCCESS -> DurableTxStatus.PENDING_SUCCESS
    DurableTxLocal.Status.FINALIZED_SUCCESS -> DurableTxStatus.FINALIZED_SUCCESS
    DurableTxLocal.Status.FAILURE -> DurableTxStatus.FAILURE
    DurableTxLocal.Status.PENDING_SUBMISSION -> DurableTxStatus.PENDING_SUBMISSION
}

private fun OwnAsset.kind() = when (this) {
    is OwnAsset.Coin -> CoinageAssetKind.COIN
    is OwnAsset.Voucher -> CoinageAssetKind.VOUCHER
}

private fun OwnAsset.index() = when (this) {
    is OwnAsset.Coin -> derivationIndex
    is OwnAsset.Voucher -> ringVrfIndex
}

private fun CoinageInput.kind() = when (this) {
    is CoinageInput.Coin -> CoinageAssetKind.COIN
    is CoinageInput.Voucher -> CoinageAssetKind.VOUCHER
}

private fun CoinageInput.indexOrNull() = when (this) {
    is CoinageInput.Coin.Own -> derivationIndex
    is CoinageInput.Coin.Received -> null
    is CoinageInput.Voucher -> ringVrfIndex
}
