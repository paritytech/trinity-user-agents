package io.paritytech.polkadotapp.app.root.presentation.root.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.android.components.ViewModelComponent
import io.paritytech.polkadotapp.app.root.domain.RealRootInteractor
import io.paritytech.polkadotapp.app.root.domain.RootInteractor

@Module
@InstallIn(ViewModelComponent::class)
interface RootActivityModule {
    @Binds
    fun bindInteractor(impl: RealRootInteractor): RootInteractor
}
