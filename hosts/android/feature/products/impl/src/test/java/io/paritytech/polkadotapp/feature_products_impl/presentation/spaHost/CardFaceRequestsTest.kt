package io.paritytech.polkadotapp.feature_products_impl.presentation.spaHost

import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.FaceShownAnswer
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ExpandedCardFaceOutcome

@OptIn(ExperimentalCoroutinesApi::class)
class CardFaceRequestsTest {
    private val requests = CardFaceRequests()

    // A closed card's session stays warm and paused: there is no screen to move, and claiming
    // success would tell the product its face is hidden when it is not.
    @Test
    fun `a request nobody is watching for is not presented`() = runTest {
        assertEquals(ExpandedCardFaceOutcome.NOT_PRESENTED, requests.setFaceShown(true))
    }

    // Only the fold knows whether the user is dragging, so the product must get the screen's answer.
    @Test
    fun `the product gets the answer the watching screen gives, in request order`() = runTest {
        val received = mutableListOf<Boolean>()
        val answers = ArrayDeque(listOf(FaceShownAnswer.APPLIED, FaceShownAnswer.USER_MOVING))
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) {
            requests.requests.collect {
                received += it.shown
                it.reply.complete(answers.removeFirst())
            }
        }

        val first = requests.setFaceShown(false)
        val second = requests.setFaceShown(true)

        assertEquals(listOf(false, true), received)
        assertEquals(ExpandedCardFaceOutcome.APPLIED, first)
        assertEquals(ExpandedCardFaceOutcome.USER_MOVING, second)
    }

    // The screen can leave between receiving a request and answering it; the product must not hang.
    @Test
    fun `a watcher that never answers leaves the request not presented`() = runTest {
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { requests.requests.collect { } }
        var outcome: ExpandedCardFaceOutcome? = null
        backgroundScope.launch { outcome = requests.setFaceShown(true) }

        advanceTimeBy(900)
        assertEquals(null, outcome)
        advanceTimeBy(200)

        assertEquals(ExpandedCardFaceOutcome.NOT_PRESENTED, outcome)
    }

    // A screen that is watching but busy must not hold the product's call open past the timeout.
    @Test
    fun `a watcher too busy to receive a request leaves it not presented`() = runTest {
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) {
            requests.requests.collect { awaitCancellation() }
        }
        backgroundScope.launch { requests.setFaceShown(true) }
        advanceTimeBy(2_000)
        var outcome: ExpandedCardFaceOutcome? = null
        backgroundScope.launch { outcome = requests.setFaceShown(false) }

        advanceTimeBy(1_100)

        assertEquals(ExpandedCardFaceOutcome.NOT_PRESENTED, outcome)
    }
}
