package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import android.net.Uri
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
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.verify
import org.mockito.Mockito.verifyNoInteractions
import org.mockito.Mockito.verifyNoMoreInteractions

class RealFundingProductsWarmUpTest {
    private val base = "getcash.dot"
    private val productId = ProductId.fromStoredValue(base)

    private val fundingDomainProvider: FundingDomainProvider = mock()
    private val resolveProductUseCase: ResolveProductUseCase = mock()
    private val dotNsResolver: DotNsResolver = mock()

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
        verifyNoMoreInteractions(dotNsResolver)
    }

    @Test
    fun `warms the base host only when the product is legacy`() = runBlocking<Unit> {
        withFundingProducts(productId)
        withResolvedExecutables(appHost = base, workerScriptUrl = null)
        withArchivesResolving()

        warmUp.warmUp()

        verifyArchiveResolvedFor(base)
        verifyNoMoreInteractions(dotNsResolver)
    }

    @Test
    fun `resolves nothing when funding config is unavailable`() = runBlocking<Unit> {
        withNoFundingConfig()

        warmUp.warmUp()

        verifyNoInteractions(resolveProductUseCase)
        verifyNoInteractions(dotNsResolver)
    }

    @Test
    fun `resolves no archive when product resolution fails`() = runBlocking<Unit> {
        withFundingProducts(productId)
        withProductResolutionFailing()

        warmUp.warmUp()

        verifyNoInteractions(dotNsResolver)
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
        whenever(fundingDomainProvider.getFundingProductIds()).thenReturn(Result.success(productIds.toSet()))
    }

    private suspend fun withNoFundingConfig() {
        whenever(fundingDomainProvider.getFundingProductIds())
            .thenReturn(Result.failure(IllegalStateException("no funding config")))
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
        whenever(resolveProductUseCase.resolve(productId)).thenReturn(Result.success(resolved))
    }

    private suspend fun withProductResolutionFailing() {
        whenever(resolveProductUseCase.resolve(productId)).thenReturn(Result.failure(IllegalStateException("registry down")))
    }

    private suspend fun withArchivesResolving() {
        whenever(dotNsResolver.resolveToLocalUri(any())).thenReturn(Result.success(mock<Uri>()))
    }

    private suspend fun withArchivesFailing() {
        whenever(dotNsResolver.resolveToLocalUri(any())).thenReturn(Result.failure(IllegalStateException("ipfs down")))
    }

    private suspend fun verifyArchiveResolvedFor(host: String) {
        verify(dotNsResolver).resolveToLocalUri(host)
    }
}
