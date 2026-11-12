package io.paritytech.polkadotapp.feature_people_impl.di

import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.network.updaters.BlockNumberUpdater
import io.paritytech.polkadotapp.chains.network.updaters.Updater
import io.paritytech.polkadotapp.chains.network.updaters.system.UpdateSystemFactory
import io.paritytech.polkadotapp.feature_account_api.data.CandidateAccount
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
import io.paritytech.polkadotapp.feature_members_api.data.updaters.MemberRecordUpdaterFactory
import io.paritytech.polkadotapp.feature_people_api.data.updaters.PeopleUpdateSystem
import io.paritytech.polkadotapp.feature_people_api.data.updaters.PeopleUpdaters
import io.paritytech.polkadotapp.feature_people_api.domain.PEOPLE

@InstallIn(SingletonComponent::class)
@Module
class PeopleProvidersModule {
    @Provides
    fun providerPeopleUpdaters(
        blockNumberUpdater: BlockNumberUpdater,
        memberRecordUpdaterFactory: MemberRecordUpdaterFactory,
        @CandidateAccount candidateAccountScope: Updater.NoChainScope<MetaAccount>,
    ): PeopleUpdaters {
        val memberRecordUpdater = memberRecordUpdaterFactory.create(
            scope = candidateAccountScope,
            collectionId = RingCollectionId.PEOPLE,
        )
        val peopleChainUpdaters = listOf(blockNumberUpdater, memberRecordUpdater)
        return PeopleUpdaters(peopleChainUpdaters)
    }

    @Provides
    fun providePeopleUpdateSystem(
        updateSystemFactory: UpdateSystemFactory,
        peopleUpdaters: PeopleUpdaters,
        knownChains: KnownChains
    ): PeopleUpdateSystem {
        val system = updateSystemFactory.createConstantSingleChain(
            updaters = peopleUpdaters.peopleChainUpdaters,
            chainId = knownChains.people
        )

        return PeopleUpdateSystem(system)
    }
}
