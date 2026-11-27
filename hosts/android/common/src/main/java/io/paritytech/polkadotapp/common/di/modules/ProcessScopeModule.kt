package io.paritytech.polkadotapp.common.di.modules

import androidx.lifecycle.ProcessLifecycleOwner
import androidx.lifecycle.lifecycleScope
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import kotlinx.coroutines.CoroutineScope
import javax.inject.Qualifier

// Runs on the main thread unless the caller adds a dispatcher.
@Qualifier
@Retention(AnnotationRetention.BINARY)
annotation class ProcessScope

@Module
@InstallIn(SingletonComponent::class)
internal object ProcessScopeModule {
    @Provides
    @ProcessScope
    fun provideProcessScope(): CoroutineScope = ProcessLifecycleOwner.get().lifecycleScope
}
