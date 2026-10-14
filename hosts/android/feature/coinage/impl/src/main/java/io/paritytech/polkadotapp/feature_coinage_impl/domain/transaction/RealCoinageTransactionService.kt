package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction

import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageAssetState
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageAssetStates
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageHandoffCommit
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageInput
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageRegistrationError
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageScheduledTransactionRequest
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionRequest
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionState
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.CoinKeypairDerivation
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.VoucherRingDerivation
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.getDerivedAccountId
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.AssetPublicKey
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.AssetRegistration
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.CoinageAssetKind
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.CoinageAssetLedger
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.LedgerAsset
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.RegistrationInput
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.RegistrationOutput
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogI
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogW
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.shortKey
import io.paritytech.polkadotapp.feature_transactions.api.data.EnrichedSendableExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableSubmission
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTransactionService
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxRegistrationError
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TxDomainId
import kotlinx.coroutines.flow.Flow
import java.util.concurrent.atomic.AtomicBoolean
import javax.inject.Inject
import javax.inject.Singleton

/** Coinage's rows of the shared ledger. Constant so it can key the oracle multibinding. */
const val COINAGE_DOMAIN_ID = "coinage"

val COINAGE_DOMAIN = TxDomainId(COINAGE_DOMAIN_ID)

/**
 * Coinage's view of the durability engine.
 *
 * The engine owns the transaction row, the submission watch and recovery; this owns the assets each
 * transaction consumes and mints, and the locks they carry. Registration of the two commits together —
 * the asset rows are written inside the engine's own write transaction — which is what keeps the rule that
 * no extrinsic is ever in flight without a record holding its inputs.
 */
@Singleton
class RealCoinageTransactionService @Inject constructor(
    private val engine: DurableTransactionService,
    private val assetLedger: CoinageAssetLedger,
    private val coinKeypairDerivation: CoinKeypairDerivation,
    private val voucherRingDerivation: VoucherRingDerivation,
    private val handoffGuard: CoinageHandoffGuard,
) : CoinageTransactionService {
    override suspend fun submitTransaction(
        extrinsic: EnrichedSendableExtrinsic,
        inputs: List<CoinageInput>,
        outputs: List<OwnAsset>,
        groupId: CoinageOperationGroupId?,
    ): Result<CoinageTransactionId> {
        coinageLogI("submit-transaction inputs=${inputs.size} outputs=${outputs.size} group=${groupId?.value}")

        val registration = runCatching { assetRegistration(inputs, outputs) }
            .getOrElse { return Result.failure(it) }

        return engine.submit(COINAGE_DOMAIN, extrinsic, groupId) { id ->
            assetLedger.registerAssets(listOf(id to registration))
        }.asCoinageError().onFailure(::logRejected)
    }

    override suspend fun submitTransactions(
        transactions: List<CoinageTransactionRequest>,
        groupId: CoinageOperationGroupId,
    ): Result<List<CoinageTransactionId>> {
        coinageLogI("submit-transactions count=${transactions.size} group=${groupId.value}")

        val registrations = runCatching { transactions.map { assetRegistration(it.inputs, it.outputs) } }
            .getOrElse { return Result.failure(it) }

        val submissions = transactions.map { DurableSubmission(it.extrinsic, it.policy) }

        return engine.submitAll(COINAGE_DOMAIN, submissions, groupId) { ids ->
            assetLedger.registerAssets(ids.zip(registrations))
        }.asCoinageError().onFailure(::logRejected)
    }

    override suspend fun scheduleTransactions(
        transactions: List<CoinageScheduledTransactionRequest>,
        groupId: CoinageOperationGroupId,
    ): Result<List<CoinageTransactionId>> {
        coinageLogI(
            "schedule-transactions count=${transactions.size} group=${groupId.value} " +
                "policies=${transactions.map { it.policy.id }.distinct()}"
        )

        val registrations = runCatching { transactions.map { assetRegistration(it.inputs, it.outputs) } }
            .getOrElse { return Result.failure(it) }

        return engine.schedule(COINAGE_DOMAIN, groupId, transactions.map { it.policy }) { ids ->
            assetLedger.registerAssets(ids.zip(registrations))
        }.onFailure(::logRejected)
    }

    override suspend fun preCommitHandoff(assets: List<OwnAsset>): Result<CoinageHandoffCommit> {
        val marks = runCatching { assets.map { LedgerAsset(it.kind(), it, it.publicKey()) } }
            .getOrElse { return Result.failure(it) }

        val keys = marks.map { it.publicKey }

        return assetLedger.markHandedOff(marks)
            .onSuccess {
                coinageLogI("handoff-marked assets=${marks.map { it.describe() }}")
                handoffGuard.handoffReserved()
            }
            .onFailure { logRejected(it) }
            .map { LedgerHandoffCommit(assetLedger, keys, handoffGuard) }
    }

    override suspend fun releaseUncommittedHandoffs(): Result<Unit> = assetLedger.releaseUncommittedHandoffs()

    override fun startRecovery() = engine.startRecovery()

    override suspend fun getTransactionStatus(id: CoinageTransactionId): Result<DurableTxStatus> =
        engine.getStatus(id)

    override fun subscribeTransactionStatus(id: CoinageTransactionId): Flow<DurableTxStatus> =
        engine.subscribeStatus(id)

    override suspend fun getOperationGroupStatuses(
        groupId: CoinageOperationGroupId,
    ): Result<List<CoinageTransactionState>> = assetLedger.getGroupStatuses(groupId)

    override fun subscribeOperationGroupStatuses(
        groupId: CoinageOperationGroupId,
    ): Flow<List<CoinageTransactionState>> = assetLedger.subscribeGroupStatuses(groupId)

    override suspend fun getAssetState(asset: OwnAsset): Result<CoinageAssetState> =
        assetLedger.getAssetState(asset)

    override suspend fun getAssetStates(assets: List<OwnAsset>): Result<CoinageAssetStates> =
        assetLedger.getAssetStates(assets)

    override fun subscribeAssetStates(): Flow<CoinageAssetStates> =
        assetLedger.subscribeAssetStates()

    private suspend fun assetRegistration(
        inputs: List<CoinageInput>,
        outputs: List<OwnAsset>,
    ) = AssetRegistration(
        inputs = inputs.map { RegistrationInput(it, it.publicKey()) },
        outputs = outputs.map { RegistrationOutput(it, it.publicKey()) },
    )

    private suspend fun CoinageInput.publicKey(): AssetPublicKey = when (this) {
        is CoinageInput.Coin.Own -> coinKeypairDerivation.getDerivedAccountId(derivationIndex)
        is CoinageInput.Coin.Received -> publicKey
        is CoinageInput.Voucher -> voucherRingDerivation.memberKeyOf(ringVrfIndex)
    }

    private suspend fun OwnAsset.publicKey(): AssetPublicKey = when (this) {
        is OwnAsset.Coin -> coinKeypairDerivation.getDerivedAccountId(derivationIndex)
        is OwnAsset.Voucher -> voucherRingDerivation.memberKeyOf(ringVrfIndex)
    }

    private fun OwnAsset.kind() = when (this) {
        is OwnAsset.Coin -> CoinageAssetKind.COIN
        is OwnAsset.Voucher -> CoinageAssetKind.VOUCHER
    }

    private fun logRejected(error: Throwable) {
        coinageLogW("registration-rejected reason=${error::class.simpleName} detail=${error.message}")
    }

    /**
     * The engine rejects a non-mortal extrinsic in its own vocabulary, but [CoinageRegistrationError] is
     * what this module publishes and what callers match on, so the two mortality rejections keep their
     * coinage names.
     */
    private fun <T> Result<T>.asCoinageError(): Result<T> = recoverCatching { error ->
        throw when (error) {
            is DurableTxRegistrationError.NotMortal -> CoinageRegistrationError.NotMortal
            is DurableTxRegistrationError.MissingEraAnchor -> CoinageRegistrationError.MissingEraAnchor
            else -> error
        }
    }
}

private class LedgerHandoffCommit(
    private val assetLedger: CoinageAssetLedger,
    private val keys: List<AssetPublicKey>,
    private val handoffGuard: CoinageHandoffGuard,
) : CoinageHandoffCommit {
    private val settled = AtomicBoolean(false)

    override suspend fun commit(): Result<Unit> = assetLedger.commitHandoffs(keys)
        .onSuccess { coinageLogI("handoff-committed keys=${keys.map { it.shortKey() }}") }
        .onFailure { error -> coinageLogW("handoff-commit-failed keys=${keys.map { it.shortKey() }} error=$error") }
        .also { settle() }

    override suspend fun release(): Result<Unit> = assetLedger.releaseUncommittedHandoffs(keys)
        .onSuccess { coinageLogI("handoff-released keys=${keys.map { it.shortKey() }}") }
        .onFailure { error -> coinageLogW("handoff-release-failed keys=${keys.map { it.shortKey() }} error=$error") }
        .also { settle() }

    /** What the guard counts is handles still deciding, so an outcome either way ends this one, once. */
    private fun settle() {
        if (settled.compareAndSet(false, true)) handoffGuard.handoffSettled()
    }
}
