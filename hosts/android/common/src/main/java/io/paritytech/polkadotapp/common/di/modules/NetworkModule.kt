package io.paritytech.polkadotapp.common.di.modules

import android.content.Context
import com.google.gson.Gson
import com.google.gson.GsonBuilder
import com.neovisionaries.ws.client.WebSocketFactory
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import io.novasama.substrate_sdk_android.wsrpc.SocketService
import io.novasama.substrate_sdk_android.wsrpc.recovery.Reconnector
import io.novasama.substrate_sdk_android.wsrpc.request.RequestExecutor
import io.paritytech.polkadotapp.common.BuildConfig
import io.paritytech.polkadotapp.common.data.network.IDENTITY_BACKEND_SENTINEL_HOST
import io.paritytech.polkadotapp.common.data.network.IdentityBackendUrlProvider
import io.paritytech.polkadotapp.common.data.network.NetworkApiCreator
import io.paritytech.polkadotapp.common.data.network.OverrideBaseUrlInterceptor
import io.paritytech.polkadotapp.common.data.network.RealNetworkApiCreator
import io.paritytech.polkadotapp.common.data.network.TestnetEnvironment
import io.paritytech.polkadotapp.common.data.network.WsConnectionLogger
import io.paritytech.polkadotapp.common.data.network.addDebugLoggingInterceptor
import okhttp3.Cache
import okhttp3.OkHttpClient
import java.io.File
import java.util.concurrent.TimeUnit
import javax.inject.Singleton

private const val HTTP_CACHE = "http_cache"
private const val CACHE_SIZE = 50L * 1024L * 1024L // 50 MiB
private const val TIMEOUT_SECONDS = 20L

private const val SOCKET_CONNECTION_TIMEOUT = 5_000

@Module
@InstallIn(SingletonComponent::class)
class NetworkModule {
    @Provides
    @Singleton
    fun provideHttpCache(@ApplicationContext context: Context): Cache =
        Cache(File(context.cacheDir, HTTP_CACHE), CACHE_SIZE)

    @Provides
    fun provideOkHttpClientBuilder(
        cache: Cache,
        identityBackendUrlProvider: IdentityBackendUrlProvider,
    ): OkHttpClient.Builder {
        val builder = OkHttpClient.Builder()
            .connectTimeout(TIMEOUT_SECONDS, TimeUnit.SECONDS)
            .writeTimeout(TIMEOUT_SECONDS, TimeUnit.SECONDS)
            .readTimeout(TIMEOUT_SECONDS, TimeUnit.SECONDS)
            .cache(cache)
            .retryOnConnectionFailure(true)
            // First, so it applies to every client built from this builder and so logging and other
            // interceptors observe the rewritten identity-backend URL.
            .addInterceptor(
                OverrideBaseUrlInterceptor(IDENTITY_BACKEND_SENTINEL_HOST) {
                    identityBackendUrlProvider.getBaseUrl().getOrThrow()
                }
            )
        return builder
    }

    @Provides
    @Singleton
    fun provideOkHttpClient(
        builder: OkHttpClient.Builder
    ): OkHttpClient {
        return builder
            .addDebugLoggingInterceptor()
            .build()
    }

    @Provides
    @Singleton
    fun provideApiCreator(
        okHttpClient: OkHttpClient,
        gson: Gson
    ): NetworkApiCreator {
        return RealNetworkApiCreator(okHttpClient, gson)
    }

    @Provides
    @Singleton
    fun provideGson(): Gson = GsonBuilder().disableHtmlEscaping().create()

    @Provides
    fun provideSocketService(jsonMapper: Gson): SocketService {
        val wsFactory =
            WebSocketFactory().apply {
                connectionTimeout = SOCKET_CONNECTION_TIMEOUT
            }

        return SocketService(
            jsonMapper = jsonMapper,
            logger = WsConnectionLogger(ignoreStateMachineLogs = true),
            webSocketFactory = wsFactory,
            reconnector = Reconnector(),
            requestExecutor = RequestExecutor()
        )
    }

    @Provides
    @Singleton
    fun provideChainEnvironment(): TestnetEnvironment {
        return TestnetEnvironment.fromNameOrDefault(BuildConfig.TESTNET_ENVIRONMENT)
    }
}
