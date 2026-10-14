package io.paritytech.polkadotapp.feature_usernames_impl.presentation.claim.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.android.components.ViewModelComponent
import dagger.hilt.android.scopes.ViewModelScoped
import io.paritytech.polkadotapp.feature_usernames_impl.domain.interactor.RealUsernamesClaimInteractor
import io.paritytech.polkadotapp.feature_usernames_impl.domain.interactor.UsernamesClaimInteractor
import io.paritytech.polkadotapp.feature_usernames_impl.domain.usecase.CreateClaimParamsUseCase
import io.paritytech.polkadotapp.feature_usernames_impl.domain.usecase.RealCreateClaimParamsUseCase

@Module
@InstallIn(ViewModelComponent::class)
interface UsernamesClaimModule {
    @Binds
    @ViewModelScoped
    fun bindsInteractor(impl: RealUsernamesClaimInteractor): UsernamesClaimInteractor

    @Binds
    @ViewModelScoped
    fun bindCreateClaimParamsUseCase(impl: RealCreateClaimParamsUseCase): CreateClaimParamsUseCase
}
