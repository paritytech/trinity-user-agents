package io.paritytech.polkadotapp.tools_auth_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.tools_auth_api.FirebaseAuthManager
import io.paritytech.polkadotapp.tools_auth_api.GoogleAuthManager
import io.paritytech.polkadotapp.tools_auth_impl.firebase.RealFirebaseAuthManager
import io.paritytech.polkadotapp.tools_auth_impl.google.RealGoogleAuthManager
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
interface GPAuthModule {
    @Binds
    @Singleton
    fun bindGoogleAuthManager(impl: RealGoogleAuthManager): GoogleAuthManager

    @Binds
    @Singleton
    fun bindFirebaseAuthManager(impl: RealFirebaseAuthManager): FirebaseAuthManager
}
