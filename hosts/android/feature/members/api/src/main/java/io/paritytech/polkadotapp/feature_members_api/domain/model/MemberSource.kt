package io.paritytech.polkadotapp.feature_members_api.domain.model

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchEntropy

sealed class MemberSource {
    class Entropy(val bandersnatchEntropy: BandersnatchEntropy) : MemberSource()

    class Account(val metaId: Long) : MemberSource()
}
