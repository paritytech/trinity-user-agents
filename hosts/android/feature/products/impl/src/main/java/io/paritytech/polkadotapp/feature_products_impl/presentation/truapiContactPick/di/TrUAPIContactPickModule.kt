package io.paritytech.polkadotapp.feature_products_impl.presentation.truapiContactPick.di

import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.components.ViewModelComponent
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ContactPickOption
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ContactPickRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIContactPicks
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIPrompt

@Module
@InstallIn(ViewModelComponent::class)
class TrUAPIContactPickModule {
    @Provides
    fun provideContactPickPrompt(picks: TrUAPIContactPicks): TrUAPIPrompt<ContactPickRequest, ContactPickOption?> =
        requireNotNull(picks.current) { "No contact pick is open." }
}
