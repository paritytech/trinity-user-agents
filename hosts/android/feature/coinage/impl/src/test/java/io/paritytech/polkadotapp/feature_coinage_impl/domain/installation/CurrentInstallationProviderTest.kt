package io.paritytech.polkadotapp.feature_coinage_impl.domain.installation

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageAccountBackupStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageAccountBackupStatus.Completed
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageAccountBackupStatus.Delayed
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageAccountBackupStatus.Registering
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_impl.TEST_INSTALLATION
import io.paritytech.polkadotapp.feature_coinage_impl.data.installation.CoinageInstallationRepository
import io.paritytech.polkadotapp.test_shared.settle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelChildren
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class CurrentInstallationProviderTest {
    private val status = MutableSharedFlow<CoinageAccountBackupStatus>(replay = 1)

    private val registrar = mockk<CoinageInstallationRegistrar> {
        every { subscribeStatus() } returns status
    }

    private val installationRepository = mockk<CoinageInstallationRepository> {
        coEvery { getOrCreateCurrent() } returns TEST_INSTALLATION
    }

    private val provider = RealCurrentInstallationProvider(registrar, installationRepository)

    @Test
    fun `a completed backup returns the installation at once`() = providerTest {
        status.emit(Completed)

        assertEquals(Result.success(TEST_INSTALLATION), provider.awaitBackedUp())
    }

    @Test
    fun `a registering backup holds the caller and does not fail`() = providerTest {
        status.emit(Registering)

        val call = request()
        settle()
        assertFalse(call.isCompleted)

        status.emit(Completed)

        assertEquals(Result.success(TEST_INSTALLATION), call.await())
    }

    @Test
    fun `a delayed backup holds the caller`() = providerTest {
        status.emit(Delayed)

        val call = request()
        settle()
        assertFalse(call.isCompleted)

        status.emit(Completed)

        assertEquals(Result.success(TEST_INSTALLATION), call.await())
    }

    @Test
    fun `moving from registering to delayed keeps the caller waiting`() = providerTest {
        status.emit(Registering)
        val call = request()
        settle()

        status.emit(Delayed)
        settle()
        assertFalse(call.isCompleted)

        status.emit(Completed)
        assertEquals(Result.success(TEST_INSTALLATION), call.await())
    }

    @Test
    fun `a flapping status holds the caller until completed`() = providerTest {
        status.emit(Registering)
        val flags = collectFlags()
        val call = request()

        listOf(Delayed, Registering).forEach {
            status.emit(it)
            settle()
            assertFalse(call.isCompleted)
        }

        status.emit(Completed)
        assertEquals(Result.success(TEST_INSTALLATION), call.await())
        settle()
        assertEquals(listOf(false, true, false), flags)
    }

    @Test
    fun `a status that leaves completed holds the next caller`() = providerTest {
        status.emit(Completed)
        provider.awaitBackedUp()

        status.emit(Registering)
        val call = request()
        settle()

        assertFalse(call.isCompleted)
        call.cancel()
    }

    @Test
    fun `a cancelled waiter is cancelled, not failed`() = providerTest {
        status.emit(Registering)
        var result: Result<CoinageInstallationId>? = null
        val call = launch(start = CoroutineStart.UNDISPATCHED) { result = provider.awaitBackedUp() }
        settle()

        call.cancel()
        settle()

        assertEquals(null, result)
    }

    @Test
    fun `the waiting flag is false while nothing requests the installation`() = providerTest {
        status.emit(Registering)

        val flags = collectFlags()
        settle()

        assertEquals(listOf(false), flags)
    }

    @Test
    fun `the waiting flag rises while a caller waits and drops when the backup completes`() = providerTest {
        status.emit(Registering)
        val flags = collectFlags()

        val call = request()
        settle()
        status.emit(Completed)
        call.await()
        settle()

        assertEquals(listOf(false, true, false), flags)
    }

    @Test
    fun `the waiting flag never rises when the backup is already completed`() = providerTest {
        status.emit(Completed)
        val flags = collectFlags()

        repeat(3) { provider.awaitBackedUp() }
        settle()

        assertEquals(listOf(false), flags)
    }

    @Test
    fun `two waiters keep the flag up until both are released`() = providerTest {
        status.emit(Registering)
        val flags = collectFlags()

        val first = request()
        val second = request()
        settle()
        first.cancel()
        settle()
        assertEquals(listOf(false, true), flags)

        status.emit(Completed)
        second.await()
        settle()

        assertEquals(listOf(false, true, false), flags)
    }

    @Test
    fun `a cancelled waiter lowers the flag`() = providerTest {
        status.emit(Registering)
        val flags = collectFlags()

        val call = request()
        settle()
        call.cancel()
        settle()

        assertEquals(listOf(false, true, false), flags)
    }

    @Test
    fun `a status flow that has not emitted yet and then emits completed never raises the flag`() = providerTest {
        val flags = collectFlags()

        val call = request()
        settle()
        status.emit(Completed)

        assertEquals(Result.success(TEST_INSTALLATION), call.await())
        settle()
        assertEquals(listOf(false), flags)
    }

    @Test
    fun `subscribeStatus forwards the registrar status`() = providerTest {
        val seen = mutableListOf<CoinageAccountBackupStatus>()
        val job = launch(start = CoroutineStart.UNDISPATCHED) { provider.subscribeStatus().toList(seen) }

        listOf(Registering, Delayed, Completed).forEach {
            status.emit(it)
            settle()
        }
        job.cancel()

        assertEquals(listOf(Registering, Delayed, Completed), seen)
    }

    private fun providerTest(body: suspend CoroutineScope.() -> Unit) = runBlocking<Unit> {
        body()
        coroutineContext.cancelChildren()
    }

    private fun CoroutineScope.request(): Deferred<Result<CoinageInstallationId>> =
        async(start = CoroutineStart.UNDISPATCHED) { provider.awaitBackedUp() }

    private fun CoroutineScope.collectFlags(): List<Boolean> {
        val flags = mutableListOf<Boolean>()
        launch(start = CoroutineStart.UNDISPATCHED) { provider.subscribeAllocationWaiting().toList(flags) }
        return flags
    }
}
