package io.paritytech.polkadotapp.app.root.domain.debug

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class ReportIssueInteractorTest {
    @get:Rule
    val temporaryFolder = TemporaryFolder()

    @Test
    fun `a sent username prefills the next report and sending without one forgets it`() = runTest {
        var stored: String? = null
        val preferences = mockk<Preferences> {
            every { getString(any()) } answers { stored }
            every { putString(any(), any()) } answers { stored = secondArg() }
        }
        val collectLogs = mockk<CollectIssueLogsUseCase> {
            coEvery { this@mockk() } answers { Result.success(temporaryFolder.newFile()) }
        }
        val interactor = ReportIssueInteractor(collectLogs, { Result.success(Unit) }, preferences, testDispatchers())
        val screenshot = temporaryFolder.newFile()

        interactor.send("The screen froze", GitHubUsername.parse("octocat"), screenshot)
        val prefilled = interactor.lastGitHubUsername()
        interactor.send("The screen froze again", null, screenshot)

        assertEquals(listOf("octocat", ""), listOf(prefilled, interactor.lastGitHubUsername()))
    }
}
