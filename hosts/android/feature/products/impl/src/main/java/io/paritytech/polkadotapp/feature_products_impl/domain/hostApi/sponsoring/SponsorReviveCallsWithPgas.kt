package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.sponsoring

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.paritytech.polkadotapp.chains.extrinsic.visitor.call.api.CallTraversal
import io.paritytech.polkadotapp.chains.extrinsic.visitor.call.api.collectLeafs
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.asDisplayString
import io.paritytech.polkadotapp.feature_balances_api.data.type.TokenBalanceTypeRegistry
import io.paritytech.polkadotapp.feature_pgas_api.domain.PgasChainAssetProvider
import io.paritytech.polkadotapp.feature_pgas_api.domain.PgasClaimSpec
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.AccountsProtocol
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocatableResource
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocationOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.OnExistingAllowancePolicy
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.requestResourceAllocation
import io.paritytech.polkadotapp.feature_products_api.domain.sponsoring.TransactionSponsoring
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.ProductAccountDerivationUseCase
import timber.log.Timber
import javax.inject.Inject

/**
 * Tops up a SmartContract (PGAS) allowance whenever the user signs an Asset Hub Revive
 * call and the signing product account's PGAS balance is below 20% of the per-claim
 * amount.
 */
class SponsorReviveCallsWithPgas @Inject constructor(
    private val knownChains: KnownChains,
    private val productAccountDerivationUseCase: ProductAccountDerivationUseCase,
    private val tokenBalanceTypeRegistry: TokenBalanceTypeRegistry,
    private val pgasChainAssetProvider: PgasChainAssetProvider,
    private val pgasClaimSpec: PgasClaimSpec,
    private val accountsProtocol: AccountsProtocol,
    private val callTraversal: CallTraversal,
) : TransactionSponsoring {
    override suspend fun sponsorTransaction(
        chainId: ChainId,
        call: GenericCall.Instance,
        account: ProductAccountId,
    ): Result<Unit> {
        if (chainId != knownChains.assetHub) return Result.success(Unit)

        return productAccountDerivationUseCase.deriveAccountId(account).mapCatching { accountId ->
            if (!callSupportsPgasSponsoring(call, accountId)) return@mapCatching

            Timber.i("Transaction by $account is eligible for sponsorship")

            val asset = pgasChainAssetProvider.asset()
            val balance = tokenBalanceTypeRegistry.typeFor(asset).getBalance(accountId)
            val claimAmount = pgasClaimSpec.currentClaimAmount(asset)
            val threshold = claimAmount * SUFFICIENT_BALANCE_PERCENT / 100

            if (balance.transferable >= threshold) {
                Timber.i("balance ${balance.transferable} ≥ threshold $threshold; already allocated")
                return@mapCatching
            }

            Timber.i("balance ${balance.transferable} < threshold $threshold; allocating SmartContract(${account.index.asDisplayString()})")
            val outcome = accountsProtocol.requestResourceAllocation(
                callingProduct = ProductId.fromStoredValue(account.productId),
                resource = ApAllocatableResource.SmartContractAllowance(account.index),
                onExisting = OnExistingAllowancePolicy.INCREASE,
            )
            when (outcome) {
                is ApAllocationOutcome.Allocated -> Unit
                ApAllocationOutcome.Rejected -> error("PGAS allocation rejected by user")
                ApAllocationOutcome.NotAvailable -> error("PGAS allocation unavailable")
            }
        }
    }

    private fun callSupportsPgasSponsoring(call: GenericCall.Instance, initialOrigin: AccountId): Boolean {
        val leafs = callTraversal.collectLeafs(source = call, initialOrigin = initialOrigin)
        return leafs.isNotEmpty() && leafs.all { it.call.module.name == Modules.REVIVE }
    }

    private companion object {
        const val SUFFICIENT_BALANCE_PERCENT = 20
    }
}
