package io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase

import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.paritytech.polkadotapp.bandersnatch_crypto.memberKey
import io.paritytech.polkadotapp.bandersnatch_crypto.sign
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.util.EncodedArguments.Companion.autoEncodedArgs
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.chains.util.call
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.VoucherAllocator
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionRequest
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.data.config.CoinageInstanceIdProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.VoucherRingDerivation
import io.paritytech.polkadotapp.feature_coinage_impl.data.signer.origins.CoinageTransactionOrigins
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionSignerSource
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.accountId
import kotlinx.serialization.Serializable
import javax.inject.Inject

interface CoinageOnboardingSubmissionUseCase {
    /**
     * Registers one voucher per denomination, minted out of [signerSource]'s external asset.
     * This does not perform any retries and returns as soon as **registration** is completed.
     * Status monitoring should be done via [CoinageTransactionService.subscribeOperationGroupStatuses] for the given [groupId]
     */
    suspend operator fun invoke(
        denominations: List<ValueExponent>,
        signerSource: TransactionSignerSource.Signed,
        chain: Chain,
        groupId: CoinageOperationGroupId,
    ): Result<Unit>
}

/**
 * Vouchers minted out of an external asset: no coinage input, one voucher output each.
 *
 * Built as one nonce-sequenced batch and registered as one unit. Either the whole attempt is in the ledger
 * or none of it is — a half-registered batch would leave vouchers on chain that nothing is tracking. What
 * becomes of each one afterwards is still decided per transaction by the recovery pass.
 */
class RealCoinageOnboardingSubmissionUseCase @Inject constructor(
    private val voucherAllocator: VoucherAllocator,
    private val voucherRingDerivation: VoucherRingDerivation,
    private val coinageOrigins: CoinageTransactionOrigins,
    private val extrinsicService: ExtrinsicService,
    private val transactionService: CoinageTransactionService,
    private val coinageInstanceIdProvider: CoinageInstanceIdProvider,
) : CoinageOnboardingSubmissionUseCase {
    override suspend fun invoke(
        denominations: List<ValueExponent>,
        signerSource: TransactionSignerSource.Signed,
        chain: Chain,
        groupId: CoinageOperationGroupId,
    ): Result<Unit> {
        // Fresh vouchers every time: the ledger refuses an output address any entry already minted, so a
        // retry cannot re-offer the ones an attempt that failed had registered.
        return voucherAllocator.allocateAll(denominations)
            .flatMap { vouchers -> register(vouchers, signerSource, chain, groupId) }
    }

    private suspend fun register(
        vouchers: List<RecyclerVoucher>,
        signerSource: TransactionSignerSource.Signed,
        chain: Chain,
        groupId: CoinageOperationGroupId,
    ): Result<Unit> {
        val txOrigin = coinageOrigins.createInfallibleUnpaidSigned(signerSource)
        val accountId = signerSource.accountId(chain)

        return coinageInstanceIdProvider.instanceId()
            .flatMap { instanceId ->
                extrinsicService.buildExtrinsics(chain, ExtrinsicService.SubmissionOptions()) {
                    vouchers.forEach { voucher ->
                        extrinsic(txOrigin) {
                            loadRecyclerWithExternalAssetUnpaid(voucher, accountId, instanceId.toLong())
                        }
                    }
                }
            }
            .flatMap { extrinsics ->
                val requests = extrinsics.mapIndexed { index, extrinsic ->
                    CoinageTransactionRequest(
                        extrinsic = extrinsic,
                        inputs = emptyList(),
                        outputs = listOf(OwnAsset.Voucher(vouchers[index].ringVrfKeyIndex)),
                        policy = null,
                    )
                }

                transactionService.submitTransactions(requests, groupId)
            }
            .coerceToUnit()
    }

    private suspend fun ExtrinsicBuilder.loadRecyclerWithExternalAssetUnpaid(
        voucher: RecyclerVoucher,
        accountId: AccountId,
        instanceId: Long,
    ) {
        val voucherEntropy = voucherRingDerivation.deriveBandersnatch(voucher.ringVrfKeyIndex)

        call(
            moduleName = Modules.COINAGE,
            callName = "load_recycler_with_external_asset_unpaid",
            arguments = autoEncodedArgs(
                "instance_id" to instanceId,
                "preservation" to Preservation.Expendable as Preservation,
                "value" to voucher.recyclerValue,
                "member_key" to voucherEntropy.memberKey(),
                "proof_of_ownership" to voucherEntropy.sign(accountId.value)
            )
        )
    }

    @Serializable
    private sealed class Preservation {
        @Serializable
        data object Expendable : Preservation()
    }
}
