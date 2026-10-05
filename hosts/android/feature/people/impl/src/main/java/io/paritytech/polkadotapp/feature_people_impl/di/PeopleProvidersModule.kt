package io.paritytech.polkadotapp.feature_people_impl.di

import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.network.updaters.BlockNumberUpdater
import io.paritytech.polkadotapp.chains.network.updaters.Updater
import io.paritytech.polkadotapp.chains.network.updaters.system.UpdateSystemFactory
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.common.data.worker.stateMachine.impl.PrefsWorkerStateMachineLocalSessionFactory
import io.paritytech.polkadotapp.feature_account_api.data.CandidateAccount
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.BandersnatchSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
import io.paritytech.polkadotapp.feature_members_api.data.updaters.MemberRecordUpdaterFactory
import io.paritytech.polkadotapp.feature_people_api.data.updaters.PeopleUpdateSystem
import io.paritytech.polkadotapp.feature_people_api.data.updaters.PeopleUpdaters
import io.paritytech.polkadotapp.feature_people_api.domain.PEOPLE
import io.paritytech.polkadotapp.feature_people_impl.data.personSetup.PersonSetupLocalSession
import io.paritytech.polkadotapp.feature_people_impl.data.personSetup.state.PersonSetupStateFactory
import io.paritytech.polkadotapp.feature_people_impl.data.storage.PersonIdStorage
import io.paritytech.polkadotapp.feature_people_impl.data.updaters.PersonAliasesUpdater
import io.paritytech.polkadotapp.feature_people_impl.data.updaters.PersonIdUpdater
import io.paritytech.polkadotapp.feature_people_impl.data.updaters.PersonRecordUpdater

@InstallIn(SingletonComponent::class)
@Module
class PeopleProvidersModule {
    @Provides
    fun providesPersonIdUpdater(
        @CandidateAccount scope: Updater.NoChainScope<MetaAccount>,
        chainRegistry: ChainRegistry,
        bandersnatchSecretsStorage: BandersnatchSecretsStorage,
        personIdStorage: PersonIdStorage,
        @RemoteSourceQualifier remoteStorageSource: StorageDataSource
    ): PersonIdUpdater = PersonIdUpdater(
        scope, chainRegistry, bandersnatchSecretsStorage, personIdStorage, remoteStorageSource
    )

    @Provides
    fun providerPeopleUpdaters(
        personIdUpdater: PersonIdUpdater,
        personRecordUpdater: PersonRecordUpdater,
        personAliasesUpdater: PersonAliasesUpdater,
        blockNumberUpdater: BlockNumberUpdater,
        memberRecordUpdaterFactory: MemberRecordUpdaterFactory,
        @CandidateAccount candidateAccountScope: Updater.NoChainScope<MetaAccount>,
    ): PeopleUpdaters {
        val memberRecordUpdater = memberRecordUpdaterFactory.create(
            scope = candidateAccountScope,
            collectionId = RingCollectionId.PEOPLE,
        )
        val peopleChainUpdaters = listOf(personIdUpdater, personRecordUpdater, blockNumberUpdater, personAliasesUpdater, memberRecordUpdater)
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

    @Provides
    fun providePersonSetupLocalSession(
        factory: PrefsWorkerStateMachineLocalSessionFactory,
        stateFactory: PersonSetupStateFactory,
    ): PersonSetupLocalSession {
        return factory.create(uniquePrefix = "PersonSetup", stateFactory)
    }
}
