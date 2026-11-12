package io.paritytech.polkadotapp.common.presentation.screens

import android.view.View
import androidx.activity.ComponentActivity
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.paritytech.polkadotapp.common.R
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import com.google.android.material.R as RMaterial

@RunWith(AndroidJUnit4::class)
class BaseBottomSheetDialogTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()

    private lateinit var scenario: ActivityScenario<ComponentActivity>
    private lateinit var dialog: BaseBottomSheetDialog

    @Before
    fun launchHost() {
        scenario = ActivityScenario.launch(ComponentActivity::class.java)
    }

    @After
    fun closeHost() {
        instrumentation.runOnMainSync { dialog.dismiss() }
        scenario.close()
    }

    @Test
    fun theSheetKeepsNoInsetPaddingOfItsOwn() {
        showDialog()

        reapplyInsets()

        val sheet = dialog.findViewById<View>(RMaterial.id.design_bottom_sheet)!!
        assertFalse(sheet.fitsSystemWindows)
        assertEquals(0, sheet.paddingBottom)
    }

    private fun showDialog() {
        scenario.onActivity { activity ->
            dialog = object : BaseBottomSheetDialog(activity, R.style.ComposeBottomSheetDialog) {}
            dialog.setContentView(View(activity).apply { minimumHeight = SHEET_CONTENT_HEIGHT })
            dialog.show()
        }
        instrumentation.waitForIdleSync()
    }

    private fun reapplyInsets() {
        instrumentation.runOnMainSync { dialog.window!!.decorView.requestApplyInsets() }
        instrumentation.waitForIdleSync()
    }

    private companion object {
        const val SHEET_CONTENT_HEIGHT = 400
    }
}
