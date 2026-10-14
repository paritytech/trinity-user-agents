package io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol

import io.paritytech.polkadotapp.chains.util.Sr25519SecretKey
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32

sealed interface ApAllocatableResource {
    data object StatementStoreAllowance : ApAllocatableResource

    data object BulletInAllowance : ApAllocatableResource

    data class SmartContractAllowance(val dest: DerivationIndex32) : ApAllocatableResource

    data object AutoSigning : ApAllocatableResource
}

sealed interface ApAllocationOutcome {
    data class Allocated(val resource: ApAllocatedResource) : ApAllocationOutcome

    data object Rejected : ApAllocationOutcome

    data object NotAvailable : ApAllocationOutcome
}

sealed interface ApAllocatedResource {
    data class StatementStoreAllowance(val slotAccountKey: SlotAccountKey) : ApAllocatedResource

    data class BulletInAllowance(val slotAccountKey: SlotAccountKey) : ApAllocatedResource

    data object SmartContractAllowance : ApAllocatedResource

    /**
     * RFC-0022: secret key of `//product//{productId}`. The hard product junction is what makes handing
     * this out safe — it exposes that product's subtree and nothing above it.
     *
     * Shape only for now: nothing produces this variant until AutoSigning allocation ships.
     */
    data class AutoSigning(val productRootSecretKey: Sr25519SecretKey) : ApAllocatedResource
}

enum class OnExistingAllowancePolicy {
    IGNORE,
    INCREASE,
}
