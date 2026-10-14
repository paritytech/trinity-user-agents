package io.paritytech.polkadotapp.common.presentation.resources

import android.content.Context
import androidx.activity.ComponentActivity
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject
import javax.inject.Singleton

interface ContextManager {
    @Deprecated("Use @ApplicationContext context: Context instead")
    val applicationContext: Context

    fun getActivity(): ComponentActivity?
    fun requireActivity(): ComponentActivity

    fun attachActivity(activity: ComponentActivity)

    fun detachActivity()
}

@Singleton
internal class RealContextManager @Inject constructor(
    @ApplicationContext override val applicationContext: Context
) : ContextManager {
    private var activity: ComponentActivity? = null

    override fun getActivity(): ComponentActivity? {
        return activity
    }

    override fun requireActivity(): ComponentActivity {
        return requireNotNull(activity)
    }

    override fun attachActivity(activity: ComponentActivity) {
        this.activity = activity
    }

    override fun detachActivity() {
        this.activity = null
    }
}
