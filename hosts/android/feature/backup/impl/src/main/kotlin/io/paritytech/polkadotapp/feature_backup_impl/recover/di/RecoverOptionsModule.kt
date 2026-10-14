package io.paritytech.polkadotapp.feature_backup_impl.recover.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.android.components.ViewModelComponent
import dagger.hilt.android.scopes.ViewModelScoped
import io.paritytech.polkadotapp.feature_backup_impl.recover.domain.RealRecoverOptionsInteractor
import io.paritytech.polkadotapp.feature_backup_impl.recover.domain.RecoverOptionsInteractor

@Module
@InstallIn(ViewModelComponent::class)
interface RecoverOptionsModule {
    @Binds
    @ViewModelScoped
    fun bindRecoverOptionsInteractor(impl: RealRecoverOptionsInteractor): RecoverOptionsInteractor
}
