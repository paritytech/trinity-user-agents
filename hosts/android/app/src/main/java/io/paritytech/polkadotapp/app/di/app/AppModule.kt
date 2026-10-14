package io.paritytech.polkadotapp.app.di.app

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.app.root.RealActivityIntentProvider
import io.paritytech.polkadotapp.app.root.RealAppLifecycleObserver
import io.paritytech.polkadotapp.app.root.data.debug.RealIssueReportApi
import io.paritytech.polkadotapp.app.root.domain.debug.IssueReportApi
import io.paritytech.polkadotapp.app.root.network.RemoteConfigIdentityBackendUrlProvider
import io.paritytech.polkadotapp.common.data.network.IdentityBackendUrlProvider
import io.paritytech.polkadotapp.common.presentation.ActivityIntentProvider
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
interface AppModule {
    @Binds
    fun bindIssueReportApi(impl: RealIssueReportApi): IssueReportApi

    @Binds
    fun bindActivityIntentProvider(impl: RealActivityIntentProvider): ActivityIntentProvider

    @Binds
    @Singleton
    fun bindAppLifecycleObserver(impl: RealAppLifecycleObserver): AppLifecycleObserver

    @Binds
    @Singleton
    fun bindIdentityBackendUrlProvider(impl: RemoteConfigIdentityBackendUrlProvider): IdentityBackendUrlProvider
}
