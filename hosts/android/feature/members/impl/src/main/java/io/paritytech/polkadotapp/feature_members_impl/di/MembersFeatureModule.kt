package io.paritytech.polkadotapp.feature_members_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_members_api.data.repository.MembersRepository
import io.paritytech.polkadotapp.feature_members_api.data.repository.MembersSubscriberRepository
import io.paritytech.polkadotapp.feature_members_api.data.updaters.MemberRecordUpdaterFactory
import io.paritytech.polkadotapp.feature_members_api.domain.CheckMemberInRingUseCase
import io.paritytech.polkadotapp.feature_members_api.domain.MembershipProver
import io.paritytech.polkadotapp.feature_members_impl.data.repository.RealMembersRepository
import io.paritytech.polkadotapp.feature_members_impl.data.repository.RealMembersSubscriberRepository
import io.paritytech.polkadotapp.feature_members_impl.data.updaters.RealMemberRecordUpdaterFactory
import io.paritytech.polkadotapp.feature_members_impl.domain.RealCheckMemberInRingUseCase
import io.paritytech.polkadotapp.feature_members_impl.domain.RealMembershipProver

@InstallIn(SingletonComponent::class)
@Module
internal interface MembersFeatureModule {
    @Binds
    fun bindMembersRepository(impl: RealMembersRepository): MembersRepository

    @Binds
    fun bindMembersSubscriberRepository(impl: RealMembersSubscriberRepository): MembersSubscriberRepository

    @Binds
    fun bindMemberRecordUpdaterFactory(impl: RealMemberRecordUpdaterFactory): MemberRecordUpdaterFactory

    @Binds
    fun bindMembershipProver(impl: RealMembershipProver): MembershipProver

    @Binds
    fun bindCheckMemberInRingUseCase(impl: RealCheckMemberInRingUseCase): CheckMemberInRingUseCase
}
