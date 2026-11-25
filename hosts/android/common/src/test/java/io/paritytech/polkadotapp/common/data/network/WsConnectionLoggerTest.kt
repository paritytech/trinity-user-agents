package io.paritytech.polkadotapp.common.data.network

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import timber.log.Timber

class WsConnectionLoggerTest {
    private val logged = mutableListOf<String>()
    private val tree = object : Timber.Tree() {
        override fun log(priority: Int, tag: String?, message: String, t: Throwable?) {
            logged += message
        }
    }
    private val logger = WsConnectionLogger(ignoreStateMachineLogs = true)

    @Before
    fun plant() {
        Timber.plant(tree)
    }

    @After
    fun uproot() {
        Timber.uprootAll()
    }

    // Light-client data usage is measured against the keys the app subscribes to, so a storage
    // subscription is logged whole, however many keys it carries; other long frames stay cut.
    @Test
    fun `a storage subscription is logged whole and other long frames are cut`() {
        val keys = List(12) { "\"0x" + "ab".repeat(80) + "\"" }.joinToString(",")
        val subscription = """[SOCKET][SENDING] {"id":1,"jsonrpc":"2.0","method":"state_subscribeStorage","params":[[$keys]]}"""
        val notification = """[SOCKET][RECEIVED] {"jsonrpc":"2.0","method":"state_storage","params":{"result":"0x${"cd".repeat(1000)}"}}"""

        logger.log(subscription)
        logger.log(notification)

        assertEquals(
            listOf(subscription, "${notification.take(1024)}... (${notification.length - 1024} more)"),
            logged,
        )
    }
}
