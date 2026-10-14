package io.paritytech.polkadotapp.feature_people_api.domain

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchAlias
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext

interface BandersnatchKeyResolver {
    suspend fun getAliasInContext(collection: PeopleCollection, context: BandersnatchContext): BandersnatchAlias
}
