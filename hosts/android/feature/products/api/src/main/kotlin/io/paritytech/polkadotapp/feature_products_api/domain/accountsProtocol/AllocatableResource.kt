package io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol

import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32

sealed interface AllocatableResource {
    data object StatementStoreAllowance : AllocatableResource

    data object BulletInAllowance : AllocatableResource

    data class SmartContractAllowance(val dest: DerivationIndex32) : AllocatableResource

    data object AutoSigning : AllocatableResource
}

sealed interface AllocationOutcome {
    data object Allocated : AllocationOutcome

    data object Rejected : AllocationOutcome

    data object NotAvailable : AllocationOutcome
}
