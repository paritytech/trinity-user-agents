package io.paritytech.polkadotapp.feature_products_api.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread

class JsGesturePositionTest {
    @Test
    fun `an answer already in returns at once`() {
        val position = JsGesturePosition()
        position.receive(10, 0)

        val started = System.nanoTime()
        val value = position.awaitAnswer(after = 0, timeoutNanos = TimeUnit.SECONDS.toNanos(5))

        assertEquals(10, value.x)
        assertTrue(System.nanoTime() - started < TimeUnit.SECONDS.toNanos(1))
    }

    @Test
    fun `an answer from another thread ends the wait`() {
        val position = JsGesturePosition()
        val waiting = CountDownLatch(1)
        val answerer = thread {
            waiting.await()
            Thread.sleep(20)
            position.receive(42, -2)
        }

        waiting.countDown()
        val value = position.awaitAnswer(after = 0, timeoutNanos = TimeUnit.SECONDS.toNanos(5))
        answerer.join()

        assertEquals(1L, value.answer)
        assertEquals(42, value.x)
        assertEquals(-2, value.y)
    }

    @Test
    fun `a late answer returns the old value once the wait is over`() {
        val position = JsGesturePosition()
        position.receive(7, 0)

        val started = System.nanoTime()
        val value = position.awaitAnswer(after = 1, timeoutNanos = TimeUnit.MILLISECONDS.toNanos(4))
        val waited = System.nanoTime() - started

        assertEquals(1L, value.answer)
        assertEquals(7, value.x)
        assertTrue("waited ${waited / 1_000} µs", waited >= TimeUnit.MILLISECONDS.toNanos(4))
    }

    @Test
    fun `a detached position stops the wait`() {
        val position = JsGesturePosition()
        val detacher = thread {
            Thread.sleep(20)
            position.detach()
        }

        val value = position.awaitAnswer(after = 0, timeoutNanos = TimeUnit.SECONDS.toNanos(5))
        detacher.join()

        assertFalse(value.attached)
        assertEquals(0L, value.answer)
    }
}
