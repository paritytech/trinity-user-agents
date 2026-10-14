package io.paritytech.polkadotapp.feature_usernames_impl.domain.model

import io.paritytech.polkadotapp.common.domain.model.EncodedPublicKey

class ClaimUsernameParams(
    val username: String,
    val preferredDigits: String,
    val candidateAddress: String,
    val candidateSignature: ByteArray,
    val consumerSignature: ByteArray,
    val membershipSignature: ByteArray,
    val ringVrfKey: ByteArray,
    val identifierKey: EncodedPublicKey,
    val dotNsSignature: ByteArray,
    val dotNsSignedAt: Long,
    val dotNsReservedUsername: String?
)
