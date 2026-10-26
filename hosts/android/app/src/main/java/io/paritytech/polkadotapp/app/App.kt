package io.paritytech.polkadotapp.app

import android.app.Application
import androidx.hilt.work.HiltWorkerFactory
import androidx.lifecycle.ProcessLifecycleOwner
import androidx.lifecycle.lifecycleScope
import androidx.work.Configuration
import coil.Coil
import coil.ImageLoader
import dagger.Lazy
import dagger.hilt.android.HiltAndroidApp
import io.paritytech.polkadotapp.app.logging.AppFileDebugTree
import io.paritytech.polkadotapp.app.logging.CoinageFileTree
import io.paritytech.polkadotapp.app.root.presentation.debug.DebugShakeObserver
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializerPipeline
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import timber.log.Timber
import javax.inject.Inject

@HiltAndroidApp
class App : Application(), Configuration.Provider {
    @Inject
    lateinit var appLifecycleObserver: AppLifecycleObserver

    @Inject
    lateinit var remoteConfigService: RemoteConfigService

    @Inject
    lateinit var imageLoader: ImageLoader

    @Inject
    lateinit var appFileDebugTree: Lazy<AppFileDebugTree>

    @Inject
    lateinit var coinageFileTree: CoinageFileTree

    @Inject
    lateinit var debugShakeObserver: DebugShakeObserver

    @Inject
    lateinit var hiltWorkerFactory: HiltWorkerFactory

    @Inject
    lateinit var appInitializerPipeline: AppInitializerPipeline

    override val workManagerConfiguration: Configuration by lazy {
        Configuration.Builder()
            .setWorkerFactory(hiltWorkerFactory)
            .build()
    }

    override fun onCreate() {
        super.onCreate()

        // Planted before the initializers: the pipeline resumes unfinished top-ups, and Timber drops
        // anything logged before a tree exists.
        Timber.plant(coinageFileTree)

        if (FeatureOption.DEBUG_MENU.isEnabled) {
            Timber.plant(Timber.DebugTree(), appFileDebugTree.get())

            ProcessLifecycleOwner.get().lifecycle
                .addObserver(debugShakeObserver)
        }

        remoteConfigService.init()

        with(ComputationalScope(ProcessLifecycleOwner.get().lifecycleScope)) {
            appInitializerPipeline.initialize()
        }

        Coil.setImageLoader(imageLoader)
        ProcessLifecycleOwner.get().lifecycle.addObserver(appLifecycleObserver)
    }
}
