package io.paritytech.polkadotapp.feature_members_api.data.updaters

import io.paritytech.polkadotapp.chains.network.updaters.Updater
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId

interface MemberRecordUpdaterFactory {
    fun create(
        scope: Updater.NoChainScope<MetaAccount>,
        collectionId: RingCollectionId,
    ): Updater<MetaAccount>
}
