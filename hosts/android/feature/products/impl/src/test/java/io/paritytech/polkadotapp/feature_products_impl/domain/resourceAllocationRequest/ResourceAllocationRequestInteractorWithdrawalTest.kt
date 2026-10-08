package io.paritytech.polkadotapp.feature_products_impl.domain.resourceAllocationRequest

import io.mockk.coEvery
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_pgas_api.domain.OnExistingAllocationStrategy
import io.paritytech.polkadotapp.feature_pgas_api.domain.PgasClaimer
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocatableResource
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocationOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.OnExistingAllowancePolicy
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.ProductAccountDerivationUseCase
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

class ResourceAllocationRequestInteractorWithdrawalTest {
    private val derivation: ProductAccountDerivationUseCase = mockk()
    private var claims = 0
    private val pgasClaimer = object : PgasClaimer {
        context(diagnostics: StalenessReportCollector)
        override suspend fun claim(destinationAccountId: AccountId, strategy: OnExistingAllocationStrategy): Result<Unit> {
            claims++
            return Result.success(Unit)
        }
    }

    private val derived = CompletableDeferred<Unit>()
    private var withdrawn = false

    private val product = ProductId.fromStoredValue("lottery.dot")
    private val resources = listOf(ApAllocatableResource.SmartContractAllowance(DerivationIndex32.default()))

    init {
        coEvery { derivation.deriveAccountId(any()) } coAnswers {
            derived.await()
            Result.success(byteArrayOf(1).toDataByteArray())
        }
    }

    @Test
    fun `an allocation is not submitted when the request is withdrawn before it`() = runTest {
        val allocation = allocateAll()
        runCurrent()

        withdrawn = true
        derived.complete(Unit)

        assertEquals(listOf(ApAllocationOutcome.NotAvailable), allocation.await().getOrThrow())
        assertClaims(0)
    }

    @Test
    fun `an allocation is submitted when nobody withdrew the request`() = runTest {
        val allocation = allocateAll()
        derived.complete(Unit)

        allocation.await()
        assertClaims(1)
    }

    private fun TestScope.allocateAll() = async {
        val interactor = ResourceAllocationRequestInteractor(
            allowanceAccountDerivation = mockk(),
            productAccountDerivationUseCase = derivation,
            transactionStorageSlotAllocator = mockk(),
            statementStoreSlotAllocator = mockk(),
            pgasClaimer = pgasClaimer,
            coroutineDispatchers = testDispatchers(),
        )
        with(StalenessReportCollector.NoOp) {
            interactor.allocateAll(product, resources, OnExistingAllowancePolicy.IGNORE) { withdrawn }
        }
    }

    private fun assertClaims(count: Int) = assertEquals("pgas claims submitted", count, claims)
}
