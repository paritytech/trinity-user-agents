package io.paritytech.polkadotapp.feature_calls_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_calls_api.domain.CallController
import io.paritytech.polkadotapp.feature_calls_api.domain.CallStateTracker
import io.paritytech.polkadotapp.feature_calls_api.domain.ProductCallMedia
import io.paritytech.polkadotapp.feature_calls_api.domain.ProductCallNotifier
import io.paritytech.polkadotapp.feature_calls_impl.notification.RealProductCallNotifier
import io.paritytech.polkadotapp.feature_calls_impl.product.RealProductCallMedia
import io.paritytech.polkadotapp.feature_calls_impl.service.RealCallController
import io.paritytech.polkadotapp.feature_calls_impl.state.CallStateHolder
import io.paritytech.polkadotapp.feature_calls_impl.state.RealCallStateTracker

@Module
@InstallIn(SingletonComponent::class)
interface CallsFeatureModule {
    @Binds
    fun bindCallController(impl: RealCallController): CallController

    @Binds
    fun bindCallStateTracker(impl: RealCallStateTracker): CallStateTracker

    @Binds
    fun bindCallStateHolder(impl: RealCallStateTracker): CallStateHolder

    @Binds
    fun bindProductCallNotifier(impl: RealProductCallNotifier): ProductCallNotifier

    @Binds
    fun bindProductCallMedia(impl: RealProductCallMedia): ProductCallMedia
}
