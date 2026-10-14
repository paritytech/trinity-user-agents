package io.paritytech.polkadotapp.app.root.presentation.debug

import android.content.Context
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.app.root.presentation.root.RootRouter
import javax.inject.Inject

class DebugShakeObserver @Inject constructor(
    @ApplicationContext context: Context,
    private val router: RootRouter
) : DefaultLifecycleObserver {
    private val shakeDetector = ShakeDetector(context)

    override fun onStart(owner: LifecycleOwner) {
        shakeDetector.register {
            router.openDebugMenu()
        }
    }

    override fun onStop(owner: LifecycleOwner) {
        shakeDetector.unregister()
    }
}
