package io.paritytech.polkadotapp.app

import android.app.Application
import android.content.ComponentName
import android.content.Context
import android.content.pm.PackageManager
import android.os.Bundle
import androidx.test.runner.AndroidJUnitRunner
import dagger.hilt.android.testing.HiltTestApplication
import io.paritytech.polkadotapp.tools_push_notifications_impl.PushNotificationService

/**
 * Instrumentation runner that swaps in [HiltTestApplication] so `@HiltAndroidTest` tests get a Hilt graph.
 */
class HiltTestRunner : AndroidJUnitRunner() {

    private lateinit var pushNotificationService: ComponentName
    private var previousPushNotificationServiceState = PackageManager.COMPONENT_ENABLED_STATE_DEFAULT

    override fun onCreate(arguments: Bundle?) {
        // FCM can start this service after a short test finishes, when HiltAndroidRule has already
        // cleared its component. None of this suite exercises push delivery, so keep the unrelated
        // service out of the instrumentation process instead of racing the rule teardown. Restore
        // the prior state in finish so a developer's device behaves exactly as it did before the run.
        pushNotificationService = ComponentName(targetContext, PushNotificationService::class.java)
        previousPushNotificationServiceState = targetContext.packageManager.getComponentEnabledSetting(
            pushNotificationService,
        )
        targetContext.packageManager.setComponentEnabledSetting(
            pushNotificationService,
            PackageManager.COMPONENT_ENABLED_STATE_DISABLED,
            PackageManager.DONT_KILL_APP,
        )
        super.onCreate(arguments)
    }

    override fun finish(resultCode: Int, results: Bundle?) {
        if (::pushNotificationService.isInitialized) {
            targetContext.packageManager.setComponentEnabledSetting(
                pushNotificationService,
                previousPushNotificationServiceState,
                PackageManager.DONT_KILL_APP,
            )
        }
        super.finish(resultCode, results)
    }

    override fun newApplication(cl: ClassLoader?, className: String?, context: Context?): Application {
        return super.newApplication(cl, HiltTestApplication::class.java.name, context)
    }
}
