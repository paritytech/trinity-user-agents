package io.paritytech.polkadotapp.tools_authentication_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.tools_authentication_api.data.AuthenticationSession
import io.paritytech.polkadotapp.tools_authentication_api.domain.BiometricsService
import io.paritytech.polkadotapp.tools_authentication_impl.data.AuthenticationMethod
import io.paritytech.polkadotapp.tools_authentication_impl.data.InMemoryAuthenticationSession
import io.paritytech.polkadotapp.tools_authentication_impl.data.methods.BiometricPromptAuthenticationMethod
import io.paritytech.polkadotapp.tools_authentication_impl.domain.RealBiometricsService
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
interface BiometricsModule {
    @Binds
    @Singleton
    fun bindBiometricsService(impl: RealBiometricsService): BiometricsService

    @Binds
    @Singleton
    fun bindAuthenticationSession(impl: InMemoryAuthenticationSession): AuthenticationSession

    @Binds
    @Singleton
    fun bindAuthenticationMethod(impl: BiometricPromptAuthenticationMethod): AuthenticationMethod
}
