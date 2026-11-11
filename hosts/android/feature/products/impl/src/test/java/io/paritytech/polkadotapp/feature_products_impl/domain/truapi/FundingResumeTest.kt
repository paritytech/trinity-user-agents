package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingRuntime
import kotlinx.coroutines.test.runTest
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

// The core leaves its resume marker in its database directory while a provider works on a session.
class FundingResumeTest {
    @get:Rule
    val tempFolder = TemporaryFolder()

    private val runtime = mockk<FundingRuntime> {
        coEvery { start() } returns Result.success(Unit)
    }
    private val context = mockk<Context> {
        every { noBackupFilesDir } answers { tempFolder.root }
    }

    @Test
    fun `starts the core at launch when the core left its resume marker`() = runTest {
        tempFolder.newFolder("truapi").resolve(RESUME_MARKER).createNewFile()

        initialize()

        coVerify(exactly = 1) { runtime.start() }
    }

    // Nothing to resume, so the core waits for a screen or product to start it.
    @Test
    fun `leaves the core alone at launch without the marker`() = runTest {
        initialize()

        coVerify(exactly = 0) { runtime.start() }
    }

    private suspend fun initialize() = FundingResume(runtime, context).resumeUnfinished()

    private companion object {
        // The core's name for the marker, which only the core writes.
        const val RESUME_MARKER = "funding-resume"
    }
}
