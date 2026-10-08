package io.paritytech.polkadotapp.feature_products_impl.domain.crossProductProof

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.utils.WithdrawableAnswer
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import java.util.UUID

class CrossProductProofContext(
    val callingProduct: ProductId,
    val onBehalfOf: ProductId,
    val suffix: DerivationIndex32,
    val message: DataByteArray,
) {
    sealed interface Decision {
        data object Approved : Decision
        data object Rejected : Decision
    }

    val id: String = UUID.randomUUID().toString()
    private val decision = WithdrawableAnswer<Decision>()

    val isWithdrawn: Boolean
        get() = decision.isWithdrawn

    fun deliverApproved() {
        decision.deliver(Decision.Approved)
    }

    fun deliverRejected() {
        decision.deliver(Decision.Rejected)
    }

    suspend fun awaitDecision(open: suspend () -> Unit): Decision = decision.await(open)

    suspend fun awaitWithdrawal() = decision.awaitWithdrawal()
}
