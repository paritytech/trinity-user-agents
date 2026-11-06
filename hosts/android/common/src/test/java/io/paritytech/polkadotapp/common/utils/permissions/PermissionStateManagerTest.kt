package io.paritytech.polkadotapp.common.utils.permissions

import android.Manifest
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test

class PermissionStateManagerTest {
    private val permissionStatusChecker = mockk<PermissionStatusChecker>()
    private val preferences = mockk<Preferences>()
    private val stored = mutableMapOf<String, Boolean>()

    private val manager = PermissionStateManager(permissionStatusChecker, preferences)

    @Before
    fun setUp() {
        every { preferences.getBoolean(any(), any()) } answers { stored[firstArg()] ?: secondArg() }
        every { preferences.putBoolean(any(), any()) } answers { stored[firstArg()] = secondArg() }
    }

    @Test
    fun `result is granted when the permission is granted`() {
        assertEquals(PermissionResult.GRANTED, manager.onPermissionResult(CAMERA, isGranted = true))
    }

    @Test
    fun `denial is not permanent when it is the first and has no rationale`() {
        withRationale(false)

        assertEquals(PermissionResult.DENIED, manager.onPermissionResult(CAMERA, isGranted = false))
    }

    @Test
    fun `denial is not permanent when it is the first and has a rationale`() {
        withRationale(true)

        assertEquals(PermissionResult.DENIED, manager.onPermissionResult(CAMERA, isGranted = false))
    }

    @Test
    fun `denial is permanent when there is no rationale after an earlier denial`() {
        withEarlierDenial()
        withRationale(false)

        assertEquals(PermissionResult.DENIED_FOREVER, manager.onPermissionResult(CAMERA, isGranted = false))
    }

    @Test
    fun `denial is permanent when neither it nor the earlier denial had a rationale`() {
        withRationale(false)
        manager.onPermissionResult(CAMERA, isGranted = false)

        assertEquals(PermissionResult.DENIED_FOREVER, manager.onPermissionResult(CAMERA, isGranted = false))
    }

    @Test
    fun `denial is not permanent when there is a rationale after an earlier denial`() {
        withEarlierDenial()
        withRationale(true)

        assertEquals(PermissionResult.DENIED, manager.onPermissionResult(CAMERA, isGranted = false))
    }

    @Test
    fun `denial is permanent when only the record of an earlier rationale exists`() {
        withRationaleShownRecord()
        withRationale(false)

        assertEquals(PermissionResult.DENIED_FOREVER, manager.onPermissionResult(CAMERA, isGranted = false))
    }

    @Test
    fun `earlier denial is forgotten when the permission is granted`() {
        withEarlierDenial()
        manager.onPermissionResult(CAMERA, isGranted = true)
        withRationale(false)

        assertEquals(PermissionResult.DENIED, manager.onPermissionResult(CAMERA, isGranted = false))
    }

    @Test
    fun `record of an earlier rationale is forgotten when the permission is granted`() {
        withRationaleShownRecord()
        manager.onPermissionResult(CAMERA, isGranted = true)
        withRationale(false)

        assertEquals(PermissionResult.DENIED, manager.onPermissionResult(CAMERA, isGranted = false))
    }

    @Test
    fun `denial is not permanent when only another permission was denied before`() {
        withEarlierDenial()
        withRationale(false)

        assertEquals(PermissionResult.DENIED, manager.onPermissionResult(MICROPHONE, isGranted = false))
    }

    @Test
    fun `state is granted when the permission is already granted`() {
        withPermissionGranted(true)

        assertEquals(PermissionResult.GRANTED, manager.getPermissionState(CAMERA))
    }

    @Test
    fun `state is permanent denial when there is no rationale after an earlier denial`() {
        withPermissionGranted(false)
        withEarlierDenial()
        withRationale(false)

        assertEquals(PermissionResult.DENIED_FOREVER, manager.getPermissionState(CAMERA))
    }

    @Test
    fun `state stays not permanent when it is only checked`() {
        withPermissionGranted(false)
        withRationale(false)
        manager.getPermissionState(CAMERA)

        assertEquals(PermissionResult.DENIED, manager.getPermissionState(CAMERA))
    }

    private fun withRationale(shouldShow: Boolean) {
        every { permissionStatusChecker.shouldShowRationale(any()) } returns shouldShow
    }

    private fun withPermissionGranted(granted: Boolean) {
        every { permissionStatusChecker.isGranted(any()) } returns granted
    }

    private fun withEarlierDenial() {
        withRationale(true)
        manager.onPermissionResult(CAMERA, isGranted = false)
    }

    private fun withRationaleShownRecord() {
        stored["permission_rationale_shown ($CAMERA)"] = true
    }

    private companion object {
        const val CAMERA = Manifest.permission.CAMERA
        const val MICROPHONE = Manifest.permission.RECORD_AUDIO
    }
}
