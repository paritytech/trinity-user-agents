package io.paritytech.polkadotapp.feature_products_impl.domain.resourceAllocationRequest

import io.paritytech.polkadotapp.common.utils.WithdrawableAnswer
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocatableResource
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocationOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.OnExistingAllowancePolicy
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import java.util.UUID
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Hand-off between the product that asked for resources and the prompt that both confirms and performs the
 * allocation. The prompt owns the work, so the outcomes - not just the user's answer - travel back through here.
 */
class ResourceAllocationRequestContext(
    val productId: ProductId,
    val resources: List<ApAllocatableResource>,
    val onExisting: OnExistingAllowancePolicy,
) {
    val id: String = UUID.randomUUID().toString()
    private val outcomes = WithdrawableAnswer<List<ApAllocationOutcome>>()

    val isWithdrawn: Boolean
        get() = outcomes.isWithdrawn

    fun deliver(outcomes: List<ApAllocationOutcome>) {
        this.outcomes.deliver(outcomes)
    }

    /**
     * Answers every resource with [outcome]. Like [deliver] it is ignored once an answer is already in, so it doubles
     * as the guard against the caller hanging forever when the prompt goes away without finishing.
     */
    fun deliverAll(outcome: ApAllocationOutcome) {
        deliver(List(resources.size) { outcome })
    }

    suspend fun awaitOutcomes(open: suspend () -> Unit): List<ApAllocationOutcome> = outcomes.await(open)

    suspend fun awaitWithdrawal() = outcomes.awaitWithdrawal()
}

@Singleton
class ResourceAllocationRequestContextHolder @Inject constructor() {
    private var context: ResourceAllocationRequestContext? = null

    fun set(context: ResourceAllocationRequestContext) {
        this.context = context
    }

    fun get(): ResourceAllocationRequestContext? = context

    // Owner-guarded: a sheet is cleared after its dismiss animation, when the holder may carry the next prompt
    fun clear(owner: ResourceAllocationRequestContext) {
        if (context === owner) {
            context = null
        }
    }
}
