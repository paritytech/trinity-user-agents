package io.paritytech.polkadotapp.feature_products_impl.presentation.spaHost

import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.CardCoverAnswer
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.CardCoverRequest
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ExpandedCardCoverOutcome

@OptIn(ExperimentalCoroutinesApi::class)
class CardCoverRequestsTest {
    private val requests = CardCoverRequests()

    // A closed card's session stays warm and paused: there is no screen to move, and claiming
    // success would tell the product its card is covered when it is not.
    @Test
    fun `a request nobody is watching for is not presented`() = runTest {
        assertEquals(ExpandedCardCoverOutcome.NOT_PRESENTED, requests.setCovered(true))
    }

    // Only the fold knows whether the user is dragging, so the product must get the screen's answer.
    @Test
    fun `the product gets the answer the watching screen gives, in request order`() = runTest {
        val received = mutableListOf<Boolean>()
        val answers = ArrayDeque(listOf(CardCoverAnswer.APPLIED, CardCoverAnswer.USER_MOVING))
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) {
            requests.requests.collect {
                received += it.covered
                it.reply.complete(answers.removeFirst())
            }
        }

        val first = requests.setCovered(true)
        val second = requests.setCovered(false)

        assertEquals(listOf(true, false), received)
        assertEquals(ExpandedCardCoverOutcome.APPLIED, first)
        assertEquals(ExpandedCardCoverOutcome.USER_MOVING, second)
    }

    // A card that leaves before it settles never moves, and the product should hear so without
    // waiting out the timeout.
    @Test
    fun `a screen that left before applying the request leaves it not presented at once`() = runTest {
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) {
            requests.requests.collect { it.reply.complete(CardCoverAnswer.NOT_PRESENTED) }
        }

        assertEquals(ExpandedCardCoverOutcome.NOT_PRESENTED, requests.setCovered(true))
        assertEquals(0L, currentTime)
    }

    // The screen can leave between receiving a request and answering it; the product must not hang.
    @Test
    fun `a watcher that never answers leaves the request not presented`() = runTest {
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { requests.requests.collect { } }
        var outcome: ExpandedCardCoverOutcome? = null
        backgroundScope.launch { outcome = requests.setCovered(false) }

        advanceTimeBy(900)
        assertEquals(null, outcome)
        advanceTimeBy(200)

        assertEquals(ExpandedCardCoverOutcome.NOT_PRESENTED, outcome)
    }

    // The product has been told its card is not there; a screen answering late must not move it after all.
    @Test
    fun `a request left unanswered is withdrawn from the screen`() = runTest {
        var received: CardCoverRequest? = null
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { requests.requests.collect { received = it } }

        requests.setCovered(true)

        assertEquals(true, received?.reply?.isCancelled)
    }

    // A screen that is watching but busy must not hold the product's call open past the timeout.
    @Test
    fun `a watcher too busy to receive a request leaves it not presented`() = runTest {
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) {
            requests.requests.collect { awaitCancellation() }
        }
        backgroundScope.launch { requests.setCovered(false) }
        advanceTimeBy(2_000)
        var outcome: ExpandedCardCoverOutcome? = null
        backgroundScope.launch { outcome = requests.setCovered(true) }

        advanceTimeBy(1_100)

        assertEquals(ExpandedCardCoverOutcome.NOT_PRESENTED, outcome)
    }
}
