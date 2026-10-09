package io.paritytech.polkadotapp.feature_products_impl.domain.permissions

import dagger.Lazy
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.database.dao.ProductPermissionGrantDao
import io.paritytech.polkadotapp.database.model.ProductPermissionGrantLocal
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.DeviceCapabilityType
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.PermissionAuthorizationEntry
import uniffi.truapi.PermissionAuthorizationRequest
import uniffi.truapi.PermissionAuthorizationStatus

@OptIn(ExperimentalCoroutinesApi::class)
class ProductPermissionRepositoryTest {
    private val product = ProductId.fromStoredValue("game.paseo")
    private val other = ProductId.fromStoredValue("other.paseo")
    private val camera = ProductPermission.DeviceCapability(DeviceCapabilityType.Camera)
    private val microphone = ProductPermission.DeviceCapability(DeviceCapabilityType.Microphone)
    private val authorized = PermissionAuthorizationStatus.AUTHORIZED
    private val denied = PermissionAuthorizationStatus.DENIED

    @Test
    fun `old native grants and legacy-only rows are visible and canonical deny wins migration`() = runTest {
        val f = Fixture()
        f.legacy.value = listOf(row(camera), row(microphone), row(ProductPermission.BalanceAccess))
        f.saved(product)[camera.canonicalRequest()!!] = denied
        f.saved(product)[ProductPermission.UserIdentityAccess.canonicalRequest()!!] = authorized

        val permissions = f.repository.getAllByProduct(product).associate { it.permission to it.granted }

        assertEquals(mapOf(camera to false, microphone to true, ProductPermission.BalanceAccess to true,
            ProductPermission.UserIdentityAccess to true), permissions)
        assertFalse(f.repository.isGranted(product, camera))
        assertTrue(f.repository.isDenied(product, camera))
        coVerify(exactly = 0) { f.dao.insert(any()) }
    }

    @Test
    fun `toggle writes canonical authority and keeps the other product unchanged`() = runTest {
        val f = Fixture()
        f.saved(product)[camera.canonicalRequest()!!] = authorized
        f.saved(other)[camera.canonicalRequest()!!] = authorized

        f.repository.revoke(product, camera)
        assertFalse(f.repository.isGranted(product, camera))
        assertTrue(f.repository.isGranted(other, camera))
        f.repository.grant(product, camera)
        assertTrue(f.repository.isGranted(product, camera))
        coVerify(exactly = 0) { f.dao.insert(any()) }
    }

    @Test
    fun `failed persistence is surfaced without claiming a successful revoke`() = runTest {
        val f = Fixture()
        f.saved(product)[camera.canonicalRequest()!!] = authorized
        coEvery { f.runtime.setPermissionAuthorizationStatus(product.value, any(), denied) } throws IllegalStateException("disk full")

        val result = runCatching { f.repository.revoke(product, camera) }

        assertEquals("disk full", result.exceptionOrNull()?.message)
        assertTrue(f.repository.isGranted(product, camera))
    }

    @Test
    fun `one use grants are invalidated by native change only for their product`() = runTest {
        val f = Fixture()
        f.repository.grantOneTime(product, camera)
        f.repository.grantOneTime(other, camera)
        f.saved(product)[camera.canonicalRequest()!!] = denied
        f.changes.changed(product.value)

        assertFalse(f.repository.consumeOneTimeGrant(product, camera))
        assertTrue(f.repository.consumeOneTimeGrant(other, camera))
        assertFalse(f.repository.consumeOneTimeGrant(other, camera))
    }

    @Test
    fun `pending legacy allow always and allow once cannot resurrect a settings revoke`() = runTest {
        for (once in listOf(false, true)) {
            val f = Fixture()
            val prompted = CompletableDeferred<Unit>()
            val answer = CompletableDeferred<Unit>()
            val request = async {
                f.repository.withPermissionRequest(product, listOf(camera)) {
                    prompted.complete(Unit)
                    answer.await()
                    if (once) f.repository.grantOneTime(product, camera) else f.repository.grant(product, camera)
                    true
                }
            }
            prompted.await()
            f.repository.revoke(product, camera)
            answer.complete(Unit)

            assertFalse(request.await())
            assertTrue(f.repository.isDenied(product, camera))
            assertFalse(f.repository.hasOneTimeGrant(product, camera))
        }
    }

    @Test
    fun `core conditional rejection fences revoke that races the platform revision check`() = runTest {
        val f = Fixture()
        coEvery { f.runtime.setPermissionAuthorizationStatusIfCurrent(product.value, any(), authorized, any()) } returns false
        assertFalse(f.repository.withPermissionRequest(product, listOf(camera)) {
            f.repository.grant(product, camera)
            true
        })
        assertFalse(f.repository.isGranted(product, camera))
    }

    @Test
    fun `native change notification refreshes permissions and settings gating without Room writes`() = runTest {
        val f = Fixture()
        val snapshots = mutableListOf<Boolean>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) {
            f.repository.observeHasAnyPermissionRequested(product).toList(snapshots)
        }
        testScheduler.runCurrent()
        assertEquals(listOf(false), snapshots)
        f.saved(product)[camera.canonicalRequest()!!] = authorized
        f.changes.changed(product.value)
        testScheduler.runCurrent()
        assertEquals(listOf(false, true), snapshots)
    }

    @Test
    fun `bundle denial retains its request identity and does not deny singleton access`() = runTest {
        val f = Fixture()
        val bundle = ProductPermission.RemotePermission.NetworkAccessSet(listOf("a.example", "b.example"))
        f.saved(product)[bundle.canonicalRequest()!!] = denied
        assertEquals(bundle, f.repository.getAllByProduct(product).single().permission)
        assertFalse(f.repository.isDenied(product, ProductPermission.RemotePermission.NetworkAccess("a.example")))
        f.repository.grant(product, bundle)
        assertEquals(authorized, f.saved(product)[bundle.canonicalRequest()!!])
    }

    @Test
    fun `reset tombstone is not treated as deny and legacy import cannot replace it`() = runTest {
        val f = Fixture()
        f.legacy.value = listOf(row(camera))
        f.saved(product)[camera.canonicalRequest()!!] = PermissionAuthorizationStatus.NOT_DETERMINED
        assertFalse(f.repository.isDenied(product, camera))
        assertFalse(f.repository.isGranted(product, camera))
    }

    @Test
    fun `domain and account mapping match canonical identifiers`() {
        assertEquals("xn--bcher-kva.example", canonicalDomain(" Bücher.example. "))
        assertEquals("*.example.com", canonicalDomain("*.Example.COM"))
        assertEquals("[::1]", canonicalDomain("[0:0:0:0:0:0:0:1]"))
        assertEquals(PermissionAuthorizationRequest.AccountAccess("other"), ProductPermission.AccountAccess("app.other.paseo").canonicalRequest())
        assertTrue(runCatching { canonicalDomain("https://example.com") }.isFailure)
    }

    @Test
    fun `JAM permission mapping preserves the full genesis and rejects short keys`() {
        val permission = ProductPermission.RemotePermission.JamPeersAccess("0x" + "ab".repeat(32))
        assertEquals(permission, permission.canonicalRequest()!!.legacyPermission())
        assertTrue(runCatching {
            ProductPermission.RemotePermission.JamPeersAccess("0xab").canonicalRequest()
        }.isFailure)
    }

    @Test
    fun `account notifications invalidate executable aliases but not other products`() {
        val changes = PermissionAuthorizationChanges()
        changes.changed("game")
        assertEquals(1L, changes.revision("app.game.paseo"))
        assertEquals(1L, changes.revision("worker.game.paseo"))
        assertEquals(0L, changes.revision("other.paseo"))
    }

    @Test
    fun `legacy-only permission reads and prompts do not require native runtime bootstrap`() = runTest {
        val f = Fixture()
        val permission = ProductPermission.BalanceAccess
        coEvery { f.dao.get(product.value, permission.typeName, permission.key) } returns row(permission)
        assertTrue(f.repository.isGranted(product, permission))
        assertTrue(f.repository.withPermissionRequest(product, listOf(permission)) {
            f.repository.grantOneTime(product, permission)
            true
        })
        assertTrue(f.repository.consumeOneTimeGrant(product, permission))
        coVerify(exactly = 0) { f.runtime.permissionAuthorizations(any()) }
    }

    @Test
    fun `historical bundle grants do not imply singleton authority`() = runTest {
        val f = Fixture()
        val bundle = ProductPermission.RemotePermission.NetworkAccessSet(listOf("a.example", "b.example"))
        val single = ProductPermission.RemotePermission.NetworkAccess("a.example")
        f.saved(product)[bundle.canonicalRequest()!!] = authorized
        assertTrue(f.repository.isGranted(product, bundle))
        assertFalse(f.repository.isGranted(product, single))
        f.saved(product)[single.canonicalRequest()!!] = denied
        assertFalse(f.repository.isGranted(product, single))
        assertTrue(f.repository.isDenied(product, single))
    }

    private fun row(permission: ProductPermission) = ProductPermissionGrantLocal(
        product.value, permission.typeName, permission.key, true, 1L,
    )

    private class Fixture {
        val dao = mockk<ProductPermissionGrantDao>()
        val runtime = mockk<TrUAPIHostRuntime>()
        private val provider = mockk<TrUAPIHostRuntimeProvider>()
        val changes = PermissionAuthorizationChanges()
        val legacy = MutableStateFlow<List<ProductPermissionGrantLocal>>(emptyList())
        private val records = mutableMapOf<String, MutableMap<PermissionAuthorizationRequest, PermissionAuthorizationStatus>>()
        private val revisions = mutableMapOf<String, ULong>()
        val repository = RealProductPermissionRepository(dao, Lazy { provider }, changes)
        fun saved(product: ProductId) = records.getOrPut(product.value) { linkedMapOf() }
        private fun entries(product: String) = records[product].orEmpty().map { PermissionAuthorizationEntry(it.key, it.value) }

        init {
            coEvery { provider.runtime() } returns Result.success(runtime)
            coEvery { dao.getAllByProduct(any()) } answers { legacy.value.filter { it.productId == firstArg<String>() } }
            every { dao.observeAllByProduct(any()) } returns legacy
            every { runtime.permissionAuthorizationRevision(any()) } answers { revisions[firstArg<String>()] ?: 0uL }
            coEvery { runtime.permissionAuthorizations(any()) } answers { entries(firstArg()) }
            coEvery { runtime.importPermissionAuthorizations(any(), any()) } answers {
                val product = firstArg<String>()
                val stored = records.getOrPut(product) { linkedMapOf() }
                secondArg<List<PermissionAuthorizationEntry>>().forEach { stored.putIfAbsent(it.request, it.status) }
                entries(product)
            }
            coEvery { runtime.setPermissionAuthorizationStatus(any(), any(), any()) } answers {
                val product = firstArg<String>()
                records.getOrPut(product) { linkedMapOf() }[secondArg()] = thirdArg()
                revisions[product] = (revisions[product] ?: 0uL) + 1uL
                changes.changed(product)
            }
            coEvery { runtime.setPermissionAuthorizationStatusIfCurrent(any(), any(), any(), any()) } answers {
                val product = firstArg<String>()
                if (arg<ULong>(3) != (revisions[product] ?: 0uL)) false else {
                    records.getOrPut(product) { linkedMapOf() }[secondArg()] = thirdArg()
                    changes.changed(product)
                    true
                }
            }
        }
    }
}
