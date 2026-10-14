package io.paritytech.polkadotapp.app.root.domain.debug

import android.content.Context
import android.content.Intent
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject
import kotlin.system.exitProcess

/**
 * Relaunches the app in a fresh process.
 *
 * Killing is the part that matters: the caller needs process-wide state gone,
 * not just a screen recreated. The relaunch intent is handed to the activity
 * manager first so it survives the process death and cold-starts; if an OEM
 * drops it the app simply closes, which is the same outcome the user is being
 * warned about anyway.
 */
class RestartAppUseCase @Inject constructor(
    @param:ApplicationContext private val context: Context,
) {
    operator fun invoke() {
        context.packageManager.getLaunchIntentForPackage(context.packageName)
            ?.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK)
            ?.let(context::startActivity)

        exitProcess(0)
    }
}
