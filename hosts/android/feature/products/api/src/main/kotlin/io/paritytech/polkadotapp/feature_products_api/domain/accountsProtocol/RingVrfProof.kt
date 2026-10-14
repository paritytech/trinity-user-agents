package io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchProof
import io.paritytech.polkadotapp.bandersnatch_crypto.ContextualAlias
import io.paritytech.polkadotapp.feature_members_api.data.model.RingIndex
import io.paritytech.polkadotapp.feature_members_api.data.model.RingRevision

data class RingVrfProof(
    val proof: BandersnatchProof,
    val contextualAlias: ContextualAlias,
    val ringIndex: RingIndex,
    val ringRevision: RingRevision,
)
