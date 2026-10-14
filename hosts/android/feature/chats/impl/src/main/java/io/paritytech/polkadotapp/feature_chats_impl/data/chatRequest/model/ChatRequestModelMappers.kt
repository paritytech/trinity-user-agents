package io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.model

import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.scale.toDomain
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.IdentityProof
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.DeviceInfoScale
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.DeviceInfo

fun IdentityProofScale.toDomain(): IdentityProof {
    return IdentityProof(
        identityAccountId = identityAccountId.intoAccountId(),
        proof = proof.toDataByteArray(),
    )
}

fun DeviceInfoScale.toDomain(): DeviceInfo {
    return DeviceInfo(
        statementAccountId = statementAccountId.intoAccountId(),
        encryptionPublicKey = encryptionPublicKey.toDomain().getOrThrow(),
    )
}
