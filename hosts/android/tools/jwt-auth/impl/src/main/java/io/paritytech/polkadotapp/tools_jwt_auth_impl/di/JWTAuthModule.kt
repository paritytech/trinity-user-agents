package io.paritytech.polkadotapp.tools_jwt_auth_impl.di

import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.common.data.keypair.ClientKeypairStore
import io.paritytech.polkadotapp.common.data.network.NetworkApiCreator
import io.paritytech.polkadotapp.common.data.network.addDebugLoggingInterceptor
import io.paritytech.polkadotapp.tools_integrity_api.interceptors.BackendIntegrityInterceptor
import io.paritytech.polkadotapp.tools_jwt_auth_api.BearerAuth
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.api.AuthTokenApi
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.interceptor.BearerTokenAuthenticator
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.interceptor.BearerTokenInterceptor
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.manager.JWTTokenProvider
import io.paritytech.polkadotapp.tools_jwt_auth_impl.domain.warmUp.JwtAuthWarmUpService
import io.paritytech.polkadotapp.tools_jwt_auth_impl.domain.warmUp.RealJwtAuthWarmUpService
import okhttp3.Dispatcher
import okhttp3.OkHttpClient
import retrofit2.Retrofit
import javax.inject.Qualifier
import javax.inject.Singleton

@Qualifier
@Retention(AnnotationRetention.RUNTIME)
private annotation class JWTAuthRetrofit

@Module
@InstallIn(SingletonComponent::class)
interface JWTAuthModule {
    @Binds
    fun bindJwtAuthWarmUpService(impl: RealJwtAuthWarmUpService): JwtAuthWarmUpService

    companion object {
        @Provides
        @Singleton
        @JWTAuthRetrofit
        fun provideRetrofit(
            backendIntegrityInterceptor: BackendIntegrityInterceptor,
            networkApiCreator: NetworkApiCreator,
            builder: OkHttpClient.Builder,
            keypairStore: ClientKeypairStore
        ): Retrofit {
            builder.addInterceptor(backendIntegrityInterceptor)
            builder.addDebugLoggingInterceptor()
            val retrofit = networkApiCreator.createRetrofit(customOkHttpClient = builder.build())
            backendIntegrityInterceptor.initRetrofit(retrofit)
            return retrofit
        }

        @Provides
        @Singleton
        fun provideAuthTokenApi(@JWTAuthRetrofit retrofit: Retrofit): AuthTokenApi =
            retrofit.create(AuthTokenApi::class.java)

        // Own Dispatcher isolates this client from the @JWTAuthRetrofit pool. Under a 401 storm,
        // BearerTokenAuthenticator does runBlocking { validToken() } on a worker here, which
        // performs HTTP I/O against AuthTokenApi (@JWTAuthRetrofit). Sharing a Dispatcher would
        // self-deadlock once all workers are blocked waiting for refresh.
        @Provides
        @Singleton
        @BearerAuth
        internal fun provideBearerOkHttpClient(
            builder: OkHttpClient.Builder,
            tokenProvider: JWTTokenProvider,
        ): OkHttpClient = builder
            .dispatcher(Dispatcher())
            .addInterceptor(BearerTokenInterceptor(tokenProvider))
            .authenticator(BearerTokenAuthenticator(tokenProvider))
            .addDebugLoggingInterceptor()
            .build()
    }
}
