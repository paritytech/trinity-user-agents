package io.paritytech.polkadotapp.feature_usernames_impl.data.claim.network.api.model

import androidx.annotation.Keep

@Keep
class UsernameClaimRequest(
    val candidateAccountId: String,
    val username: String,
    val candidateSignature: String,
    val ringVrfKey: String,
    val proofOfOwnership: String,
    val consumerRegistrationSignature: String,
    val identifierKey: String,
    val preferredDigits: String?,
    val dotns: UsernameClaimDotNsRequest?,
)

@Keep
class UsernameClaimDotNsRequest(
    val signature: String,
    val signedAt: Long,
    val reservedUsername: String?
)
