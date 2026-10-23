package io.paritytech.polkadotapp.app

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.work.testing.WorkManagerTestInitHelper
import dagger.hilt.android.testing.HiltAndroidRule
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import kotlinx.coroutines.runBlocking
import org.junit.Before
import org.junit.Rule
import javax.inject.Inject

abstract class BaseIntegrationTest {

    @get:Rule
    val hiltRule = HiltAndroidRule(this)

    @Inject lateinit var remoteConfigService: RemoteConfigService

    @Before
    fun setUp() = runBlocking<Unit> {
        // HiltTestApplication does not implement Configuration.Provider, so WorkManager — touched while
        // the Hilt graph is built — must be initialized explicitly before injection.
        WorkManagerTestInitHelper.initializeTestWorkManager(ApplicationProvider.getApplicationContext<Context>())
        hiltRule.inject()

        remoteConfigService.init()
        remoteConfigService.sync()
    }
}
