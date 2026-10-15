package io.paritytech.polkadotapp.common.utils

import android.app.Activity
import android.app.Application
import android.content.Intent
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResult
import androidx.activity.result.ActivityResultCallback
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.ActivityResultRegistry
import androidx.activity.result.contract.ActivityResultContract
import io.mockk.every
import io.mockk.justRun
import io.mockk.mockk
import io.mockk.slot
import io.mockk.verify
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class ActivityResultExecutorTest {
    private val callbacks = slot<Application.ActivityLifecycleCallbacks>()
    private val application: Application = mockk {
        justRun { registerActivityLifecycleCallbacks(capture(callbacks)) }
        justRun { unregisterActivityLifecycleCallbacks(any()) }
    }

    private class Registration(val key: String, val callback: ActivityResultCallback<ActivityResult>)

    private class Screen(application: Application) {
        val registrations = mutableListOf<Registration>()
        val launcher: ActivityResultLauncher<Intent> = mockk(relaxed = true)
        val registry: ActivityResultRegistry = mockk {
            every { register(any(), any<ActivityResultContract<Intent, ActivityResult>>(), any()) } answers {
                registrations += Registration(firstArg(), thirdArg())
                launcher
            }
        }
        val activity: ComponentActivity = mockk {
            every { activityResultRegistry } returns registry
            every { this@mockk.application } returns application
        }
    }

    private class Executor(activity: ComponentActivity) : ActivityResultExecutor<Int>(activity) {
        override fun createIntent(): Intent = mockk()
        override fun handleResult(result: ActivityResult): Result<Int> = Result.success(result.resultCode)
    }

    // A configuration change while the picker runs makes the activity again, and the result goes to the new one.
    @Test
    fun `a result that comes to an activity made again still reaches the caller`() = runTest {
        val first = Screen(application)
        val pending = async(start = CoroutineStart.UNDISPATCHED) { Executor(first.activity).execute() }
        verify { first.launcher.launch(any()) }

        val second = Screen(application)
        callbacks.captured.onActivityCreated(second.activity, null)
        second.registrations.single().callback.onActivityResult(ActivityResult(Activity.RESULT_OK, null))

        assertEquals(Activity.RESULT_OK, pending.await().getOrThrow())
        assertEquals(first.registrations.single().key, second.registrations.single().key)
        verify { application.unregisterActivityLifecycleCallbacks(callbacks.captured) }
    }

    @Test
    fun `the result of the first activity ends the wait`() = runTest {
        val first = Screen(application)
        val pending = async(start = CoroutineStart.UNDISPATCHED) { Executor(first.activity).execute() }

        first.registrations.single().callback.onActivityResult(ActivityResult(Activity.RESULT_CANCELED, null))

        assertEquals(Activity.RESULT_CANCELED, pending.await().getOrThrow())
        assertFalse(pending.isCancelled)
    }
}
