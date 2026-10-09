package io.paritytech.polkadotapp.feature_products_impl.domain.operation

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.data.repository.FundingOperationRecord
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductFundingOperationRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.FakeChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorkerRefCounter
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorkerReference
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.WorkerModalityApi
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger

class ProductOperationServiceTest {
    private val productId = ProductId.fromStoredValue("coinflip.dot")

    private class FakeRefCounter : ProductWorkerRefCounter {
        val acquires = AtomicInteger(0)
        val releases = AtomicInteger(0)

        override suspend fun acquire(productId: ProductId, label: String): ProductWorkerReference {
            acquires.incrementAndGet()
            return object : ProductWorkerReference {
                private val released = AtomicBoolean(false)
                override suspend fun worker(): ProductWorker = error("unused")
                override suspend fun enableModalityApi(api: WorkerModalityApi) = error("unused")
                override fun release() {
                    if (released.compareAndSet(false, true)) releases.incrementAndGet()
                }
            }
        }

        override fun chatMessaging(productId: ProductId): ProductChatMessaging = FakeChatMessaging()
    }

    private class FakeRepository : ProductFundingOperationRepository {
        val saved = mutableListOf<FundingOperationRecord>()
        var failInsert = false
        var failDelete = false
        private var nextId = 1L

        override suspend fun insert(productId: ProductId, label: String?): Result<OperationId> {
            if (failInsert) return Result.failure(RuntimeException("insert"))
            val id = OperationId(nextId++)
            saved += FundingOperationRecord(productId, id, label)
            return Result.success(id)
        }

        override suspend fun delete(id: OperationId): Result<Unit> {
            if (failDelete) return Result.failure(RuntimeException("delete"))
            saved.removeAll { it.id == id }
            return Result.success(Unit)
        }

        override suspend fun loadAll(): List<FundingOperationRecord> = saved.toList()
    }

    @Test
    fun `begin assigns distinct non-zero ids, acquires, and persists`() = runTest {
        val refCounter = FakeRefCounter()
        val repo = FakeRepository()
        val service = RealProductOperationService(refCounter, repo)

        val id1 = service.begin(productId, "a").getOrThrow()
        val id2 = service.begin(productId, "b").getOrThrow()

        assertNotEquals(0L, id1.value)
        assertNotEquals(id1, id2)
        assertEquals(2, refCounter.acquires.get())
        assertEquals(2, repo.saved.size)
    }

    @Test
    fun `end releases, deletes, and is idempotent for repeated ids`() = runTest {
        val refCounter = FakeRefCounter()
        val repo = FakeRepository()
        val service = RealProductOperationService(refCounter, repo)

        val id = service.begin(productId, null).getOrThrow()
        assertTrue(service.end(productId, id).isSuccess)

        assertEquals(1, refCounter.releases.get())
        assertTrue(repo.saved.isEmpty())

        assertTrue(service.end(productId, id).isSuccess)
        assertTrue(service.end(productId, OperationId(999L)).isSuccess)
        assertEquals(1, refCounter.releases.get())
    }

    @Test
    fun `persistence failure on begin rolls back the worker acquisition`() = runTest {
        val refCounter = FakeRefCounter()
        val repo = FakeRepository().apply { failInsert = true }
        val service = RealProductOperationService(refCounter, repo)

        assertTrue(service.begin(productId, "x").isFailure)
        assertEquals(1, refCounter.acquires.get())
        assertEquals("acquisition rolled back", 1, refCounter.releases.get())
    }

    @Test
    fun `delete failure on end still releases the worker`() = runTest {
        val refCounter = FakeRefCounter()
        val repo = FakeRepository()
        val service = RealProductOperationService(refCounter, repo)

        val id = service.begin(productId, null).getOrThrow()
        repo.failDelete = true

        assertTrue(service.end(productId, id).isSuccess)
        assertEquals("released despite delete failure", 1, refCounter.releases.get())
    }

    @Test
    fun `resume re-acquires the worker for each persisted operation`() = runTest {
        val refCounter = FakeRefCounter()
        val repo = FakeRepository()
        // Simulate records left open by a previous run.
        val id1 = repo.insert(productId, "a").getOrThrow()
        repo.insert(ProductId.fromStoredValue("arena.dot"), "b").getOrThrow()

        val service = RealProductOperationService(refCounter, repo)
        service.resumeOpenOperations()

        assertEquals(2, refCounter.acquires.get())
        // A resumed operation ends normally, releasing its worker.
        assertTrue(service.end(productId, id1).isSuccess)
        assertEquals(1, refCounter.releases.get())
    }
}
