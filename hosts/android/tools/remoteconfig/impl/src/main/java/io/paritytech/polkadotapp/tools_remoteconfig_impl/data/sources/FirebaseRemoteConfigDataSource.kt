package io.paritytech.polkadotapp.tools_remoteconfig_impl.data.sources

import com.google.firebase.Firebase
import com.google.firebase.remoteconfig.ConfigUpdate
import com.google.firebase.remoteconfig.ConfigUpdateListener
import com.google.firebase.remoteconfig.CustomSignals
import com.google.firebase.remoteconfig.FirebaseRemoteConfigException
import com.google.firebase.remoteconfig.remoteConfig
import com.google.firebase.remoteconfig.remoteConfigSettings
import io.paritytech.polkadotapp.common.BuildConfig
import io.paritytech.polkadotapp.common.data.network.TestnetEnvironment
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.tools_common.executeSuspend
import io.paritytech.polkadotapp.tools_remoteconfig_impl.R
import io.paritytech.polkadotapp.tools_remoteconfig_impl.data.RemoteConfigDataSource
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.shareIn
import timber.log.Timber
import javax.inject.Inject

class FirebaseRemoteConfigDataSource @Inject constructor(
    coroutineDispatchers: CoroutineDispatchers,
    private val testnetEnvironment: TestnetEnvironment,
) : RemoteConfigDataSource {
    private val remoteConfig = Firebase.remoteConfig
    private val scope = CoroutineScope(SupervisorJob() + coroutineDispatchers.io)

    private val configUpdates: Flow<Set<String>> = callbackFlow {
        val registration = remoteConfig.addOnConfigUpdateListener(object : ConfigUpdateListener {
            override fun onUpdate(configUpdate: ConfigUpdate) {
                remoteConfig.activate().addOnCompleteListener { trySend(configUpdate.updatedKeys) }
            }

            override fun onError(error: FirebaseRemoteConfigException) {
                Timber.w(error, "RemoteConfig realtime listener error")
            }
        })

        awaitClose { registration.remove() }
    }.shareIn(scope, SharingStarted.WhileSubscribed(), replay = 0)

    override fun init() {
        val configSettings = remoteConfigSettings {
            minimumFetchIntervalInSeconds = 0
        }
        remoteConfig.setConfigSettingsAsync(configSettings)
        remoteConfig.setDefaultsAsync(R.xml.remote_config_defaults)
    }

    override suspend fun sync(): Result<Unit> {
        return setEnvironmentSignal()
            .flatMap { remoteConfig.fetchAndActivate().executeSuspend() }
            .coerceToUnit()
    }

    override suspend fun getString(key: String): String {
        return remoteConfig.getString(key)
    }

    override suspend fun getBoolean(key: String): Boolean {
        return remoteConfig.getBoolean(key)
    }

    override suspend fun getLong(key: String): Long {
        return remoteConfig.getLong(key)
    }

    override fun configUpdates(): Flow<Set<String>> = configUpdates

    private suspend fun setEnvironmentSignal(): Result<Unit> {
        val signals = CustomSignals.Builder()
            .put(ENVIRONMENT_SIGNAL, testnetEnvironment.toEnvironmentSignal())
            .build()
        return remoteConfig.setCustomSignals(signals).executeSuspend().coerceToUnit()
    }

    private fun TestnetEnvironment.toEnvironmentSignal(): String {
        return when (this) {
            TestnetEnvironment.TESTNET -> "unstable"
            TestnetEnvironment.NIGHTLY -> if (BuildConfig.SAFETY_MODE) "safetynet" else "nightly"
            TestnetEnvironment.PRODUCTION -> "release"
        }
    }

    private companion object {
        const val ENVIRONMENT_SIGNAL = "environment"
    }
}
