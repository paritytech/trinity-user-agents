package io.paritytech.polkadotapp.tools_integrity_impl.di

import com.google.android.play.core.integrity.IntegrityManager
import com.google.android.play.core.integrity.IntegrityManagerFactory
import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.common.data.network.NetworkApiCreator
import io.paritytech.polkadotapp.common.data.network.create
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.tools_integrity_api.claim.ClaimDeviceEvidenceProvider
import io.paritytech.polkadotapp.tools_integrity_api.interceptors.BackendIntegrityInterceptor
import io.paritytech.polkadotapp.tools_integrity_api.interceptors.WidevineIntegrityInterceptor
import io.paritytech.polkadotapp.tools_integrity_impl.data.api.IntegrityApi
import io.paritytech.polkadotapp.tools_integrity_impl.data.claim.RealClaimDeviceEvidenceProvider
import io.paritytech.polkadotapp.tools_integrity_impl.data.integrity.PlayIntegrityManager
import io.paritytech.polkadotapp.tools_integrity_impl.data.integrity.RealPlayIntegrityManager
import io.paritytech.polkadotapp.tools_integrity_impl.data.interceptors.RealBackendIntegrityInterceptor
import io.paritytech.polkadotapp.tools_integrity_impl.data.interceptors.RealWidevineIntegrityInterceptor

@Module
@InstallIn(SingletonComponent::class)
interface IntegrityModule {
    companion object {
        @Provides
        fun provideIntegrityManager(
            contextManager: ContextManager,
        ): IntegrityManager {
            return IntegrityManagerFactory.create(contextManager.applicationContext)
        }

        @Provides
        fun provideIntegrityApi(networkApiCreator: NetworkApiCreator): IntegrityApi {
            return networkApiCreator.create()
        }
    }

    @Binds
    fun bindBackendIntegrityInterceptor(impl: RealBackendIntegrityInterceptor): BackendIntegrityInterceptor

    @Binds
    fun bindWidevineIntegrityInterceptor(impl: RealWidevineIntegrityInterceptor): WidevineIntegrityInterceptor

    @Binds
    fun bindClaimDeviceEvidenceProvider(impl: RealClaimDeviceEvidenceProvider): ClaimDeviceEvidenceProvider

    @Binds
    fun bindPlayIntegrityManager(impl: RealPlayIntegrityManager): PlayIntegrityManager
}
