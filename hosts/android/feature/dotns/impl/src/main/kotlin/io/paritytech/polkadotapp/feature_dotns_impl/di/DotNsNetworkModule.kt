package io.paritytech.polkadotapp.feature_dotns_impl.di

import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import okhttp3.Dispatcher
import okhttp3.OkHttpClient
import java.util.concurrent.TimeUnit
import javax.inject.Qualifier
import javax.inject.Singleton

/** Marks the dedicated [OkHttpClient] used to download CAR archives. */
@Qualifier
@Retention(AnnotationRetention.BINARY)
internal annotation class DotNsHttpClient

@Module
@InstallIn(SingletonComponent::class)
internal object DotNsNetworkModule {
    /**
     * Dedicated client for downloading CAR archives, which can be hundreds of MB. It differs from the
     * shared app client in three ways that matter for large streaming downloads:
     *
     * - **No body logging.** It is built from the shared [OkHttpClient.Builder], which does not include
     *   the debug `HttpLoggingInterceptor` (that is added only when the shared client is finalized).
     *   BODY-level logging buffers the whole response into memory and OOMs on large archives.
     * - **No response cache.** The shared 50 MiB disk cache would copy every archive a second time and
     *   thrash; archives are already persisted, unpacked, on disk by the content storage.
     * - **No read timeout.** A multi-minute download must not trip the shared 20s default.
     *
     * A private single-request dispatcher keeps these heavy downloads off the shared connection executor.
     */
    @Provides
    @Singleton
    @DotNsHttpClient
    fun provideDotNsOkHttpClient(builder: OkHttpClient.Builder): OkHttpClient {
        return builder
            .cache(null)
            .readTimeout(0, TimeUnit.SECONDS)
            .dispatcher(Dispatcher().apply { maxRequests = 1 })
            .build()
    }
}
