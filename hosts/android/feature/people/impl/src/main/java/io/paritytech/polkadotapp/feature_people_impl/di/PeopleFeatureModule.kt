package io.paritytech.polkadotapp.feature_people_impl.di

import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.paritytech.polkadotapp.feature_people_api.domain.BandersnatchKeyResolver
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCheckMemberInRingUseCase
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleMembershipProver
import io.paritytech.polkadotapp.feature_people_api.domain.useCase.ActivePeopleCollectionUseCase
import io.paritytech.polkadotapp.feature_people_api.domain.useCase.PersonStatusUseCase
import io.paritytech.polkadotapp.feature_people_impl.domain.RealBandersnatchKeyResolver
import io.paritytech.polkadotapp.feature_people_impl.domain.RealPeopleCheckMemberInRingUseCase
import io.paritytech.polkadotapp.feature_people_impl.domain.RealPeopleMembershipProver
import io.paritytech.polkadotapp.feature_people_impl.domain.useCase.DisabledPersonStatusUseCase
import io.paritytech.polkadotapp.feature_people_impl.domain.useCase.RealActivePeopleCollectionUseCase
import io.paritytech.polkadotapp.feature_people_impl.domain.useCase.RealPersonStatusUseCase
import javax.inject.Singleton

@InstallIn(SingletonComponent::class)
@Module
internal interface PeopleFeatureModule {
    @Binds
    fun bindPeopleMembershipProver(impl: RealPeopleMembershipProver): PeopleMembershipProver

    @Binds
    fun bindPeopleCheckMemberInRingUseCase(impl: RealPeopleCheckMemberInRingUseCase): PeopleCheckMemberInRingUseCase

    @Binds
    fun bindActivePeopleCollectionUseCase(impl: RealActivePeopleCollectionUseCase): ActivePeopleCollectionUseCase

    @Binds
    fun bindBandersnatchKeyResolver(impl: RealBandersnatchKeyResolver): BandersnatchKeyResolver

    companion object {
        @Provides
        @Singleton
        fun providePersonStatusUseCase(
            enabled: RealPersonStatusUseCase,
            disabled: DisabledPersonStatusUseCase
        ): PersonStatusUseCase {
            return if (FeatureOption.PERSONHOOD.isEnabled) enabled else disabled
        }
    }
}
