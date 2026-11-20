package io.parity.truapi

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.util.UUID
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.truapi.PublicProfile
import uniffi.truapi.PublicProfileException
import uniffi.truapi.PublicProfileResolutionOptions

@RunWith(AndroidJUnit4::class)
class PublicProfileStreamTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val directory = context.noBackupFilesDir.resolve("truapi-test-${UUID.randomUUID()}")

    @After
    fun cleanUp() {
        directory.deleteRecursively()
    }

    @Test
    fun anInvalidUsernameEndsTheStreamWithATypedErrorBeforeAnyProfile() {
        directory.mkdirs()
        val runtime = TrUAPIHostRuntime(InertHostBridge(context), instrumentedRuntimeConfig(directory.absolutePath))
        val profiles = mutableListOf<PublicProfile>()

        runtime.use {
            assertThrows(PublicProfileException.InvalidUsername::class.java) {
                runBlocking {
                    withTimeout(5_000) {
                        it.observePublicProfile(
                            "invalid username",
                            PublicProfileResolutionOptions(forceFresh = false, skipStale = false),
                        ).collect { profile -> profiles += profile }
                    }
                }
            }
        }

        assertEquals(emptyList<PublicProfile>(), profiles)
    }
}
