package io.paritytech.polkadotapp.feature_members_api.domain

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchAlias
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchProof
import io.paritytech.polkadotapp.feature_members_api.data.model.RingIndex
import io.paritytech.polkadotapp.feature_members_api.data.model.RingRevision

data class RingVrfMembershipProof(
    val proof: BandersnatchProof,
    val alias: BandersnatchAlias,
    val ringIndex: RingIndex,
    val ringRevision: RingRevision,
)

sealed class RingVrfProofError(message: String, cause: Throwable? = null) : Throwable(message, cause) {
    data object NotMember : RingVrfProofError("Member key is not included in the requested ring")
    data object RingNotFound : RingVrfProofError("Ring root not found for the requested ring")
    class DataFetchFailed(cause: Throwable) : RingVrfProofError("Failed to fetch ring data", cause)
}
