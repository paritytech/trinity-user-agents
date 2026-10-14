package io.paritytech.polkadotapp.feature_chain_resources_api.data.api

import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.paritytech.polkadotapp.chains.util.EncodedArguments.Companion.autoEncodedArgs
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.chains.util.call
import io.paritytech.polkadotapp.common.data.substrate.model.MultiSignature
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chain_resources_api.data.model.UsernameChoice

@JvmInline
value class ResourcesCalls(val extrinsicBuilder: ExtrinsicBuilder)

val ExtrinsicBuilder.resourcesCalls: ResourcesCalls
    get() = ResourcesCalls(this)

fun ResourcesCalls.removeReservation(username: String, account: AccountId) {
    extrinsicBuilder.call(
        moduleName = Modules.RESOURCES,
        callName = "remove_expired_username_reservation",
        arguments = autoEncodedArgs(
            "username" to username,
            "account" to account
        )
    )
}

fun ResourcesCalls.registerPerson(
    accountId: AccountId,
    liteIdentityProof: MultiSignature.Sr25519,
    usernameChoice: UsernameChoice
) {
    extrinsicBuilder.call(
        moduleName = Modules.RESOURCES,
        callName = "register_person",
        arguments = autoEncodedArgs<_, _, MultiSignature>(
            "linked_lite_identity" to accountId,
            "username_choice" to usernameChoice,
            "lite_identity_proof" to liteIdentityProof,
        )
    )
}

fun ResourcesCalls.setStatementStoreAccount(
    period: UInt,
    seq: UInt,
    targetAccount: AccountId,
) {
    extrinsicBuilder.call(
        moduleName = Modules.RESOURCES,
        callName = "set_statement_store_account",
        arguments = autoEncodedArgs(
            "period" to period,
            "seq" to seq,
            "target_account" to targetAccount,
        )
    )
}

fun ResourcesCalls.claimLongTermStorage(
    period: UInt,
    counter: UByte,
    accountId: AccountId,
) {
    extrinsicBuilder.call(
        moduleName = Modules.RESOURCES,
        callName = "claim_long_term_storage",
        arguments = autoEncodedArgs(
            "period" to period,
            "counter" to counter,
            "account_id" to accountId,
        )
    )
}

fun ResourcesCalls.setStmtStoreAssociatedAccountIdAtSlot(
    slotNumber: Byte,
    setAccount: AccountId?,
) {
    extrinsicBuilder.call(
        moduleName = Modules.RESOURCES,
        callName = "set_stmt_store_associated_account_id_at_slot",
        arguments = autoEncodedArgs(
            "slot_number" to slotNumber,
            "set_account" to setAccount,
        )
    )
}
