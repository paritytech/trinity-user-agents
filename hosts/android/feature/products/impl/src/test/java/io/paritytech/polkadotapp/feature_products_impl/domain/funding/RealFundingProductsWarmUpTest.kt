package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import android.net.Uri
import io.mockk.Called
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.confirmVerified
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_products_api.domain.FundingDomainProvider
import io.paritytech.polkadotapp.feature_products_api.model.ExecutableHost
import io.paritytech.polkadotapp.feature_products_api.model.Executables
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_api.model.SemVer
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import kotlinx.coroutines.runBlocking
import org.junit.Test

class RealFundingProductsWarmUpTest {
    private val base = "getcash.dot"
    private val productId = ProductId.fromStoredValue(base)

    private val fundingDomainProvider: FundingDomainProvider = mockk()
    private val resolveProductUseCase: ResolveProductUseCase = mockk()
    private val dotNsResolver: DotNsResolver = mockk()

    private val warmUp = RealFundingProductsWarmUp(
        fundingDomainProvider = fundingDomainProvider,
        resolveProductUseCase = resolveProductUseCase,
        dotNsResolver = dotNsResolver,
    )

    @Test
    fun `warms app and worker hosts when the product has a manifest`() = runBlocking<Unit> {
        withFundingProducts(productId)
        withResolvedExecutables(appHost = "app.$base", workerScriptUrl = "https://worker.$base/index.js")
        withArchivesResolving()

        warmUp.warmUp()

        verifyArchiveResolvedFor("app.$base")
        verifyArchiveResolvedFor("worker.$base")
        confirmVerified(dotNsResolver)
    }

    @Test
    fun `warms the base host only when the product is legacy`() = runBlocking<Unit> {
        withFundingProducts(productId)
        withResolvedExecutables(appHost = base, workerScriptUrl = null)
        withArchivesResolving()

        warmUp.warmUp()

        verifyArchiveResolvedFor(base)
        confirmVerified(dotNsResolver)
    }

    @Test
    fun `resolves nothing when funding config is unavailable`() = runBlocking<Unit> {
        withNoFundingConfig()

        warmUp.warmUp()

        verify { resolveProductUseCase wasNot Called }
        verify { dotNsResolver wasNot Called }
    }

    @Test
    fun `resolves no archive when product resolution fails`() = runBlocking<Unit> {
        withFundingProducts(productId)
        withProductResolutionFailing()

        warmUp.warmUp()

        verify { dotNsResolver wasNot Called }
    }

    @Test
    fun `completes when an archive resolution fails`() = runBlocking<Unit> {
        withFundingProducts(productId)
        withResolvedExecutables(appHost = "app.$base", workerScriptUrl = "https://worker.$base/index.js")
        withArchivesFailing()

        warmUp.warmUp()

        verifyArchiveResolvedFor("app.$base")
        verifyArchiveResolvedFor("worker.$base")
    }

    private suspend fun withFundingProducts(vararg productIds: ProductId) {
        coEvery { fundingDomainProvider.getFundingProductIds() } returns Result.success(productIds.toSet())
    }

    private suspend fun withNoFundingConfig() {
        coEvery { fundingDomainProvider.getFundingProductIds() } returns Result.failure(IllegalStateException("no funding config"))
    }

    private suspend fun withResolvedExecutables(appHost: String, workerScriptUrl: String?) {
        val executables = Executables(
            app = ProductExecutable.App(host = ExecutableHost(appHost), appVersion = SemVer.ZERO),
            widget = null,
            worker = workerScriptUrl?.let {
                ProductExecutable.Worker(
                    scriptUrl = it,
                    appVersion = SemVer.ZERO,
                    includesChat = true,
                    includesPocket = false,
                    pocketCards = emptyList(),
                )
            },
        )
        val resolved = ResolvedProduct(
            product = Product(id = productId, name = base, icon = null),
            executables = executables,
        )
        coEvery { resolveProductUseCase.resolve(productId) } returns Result.success(resolved)
    }

    private suspend fun withProductResolutionFailing() {
        coEvery { resolveProductUseCase.resolve(productId) } returns Result.failure(IllegalStateException("registry down"))
    }

    private suspend fun withArchivesResolving() {
        coEvery { dotNsResolver.resolveToLocalUri(any()) } returns Result.success(mockk<Uri>())
    }

    private suspend fun withArchivesFailing() {
        coEvery { dotNsResolver.resolveToLocalUri(any()) } returns Result.failure(IllegalStateException("ipfs down"))
    }

    private suspend fun verifyArchiveResolvedFor(host: String) {
        coVerify(exactly = 1) { dotNsResolver.resolveToLocalUri(host) }
    }
}
