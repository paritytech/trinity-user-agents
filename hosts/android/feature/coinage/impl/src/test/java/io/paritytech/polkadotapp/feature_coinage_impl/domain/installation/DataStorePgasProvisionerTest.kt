package io.paritytech.polkadotapp.feature_coinage_impl.domain.installation

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.feature_balances_api.data.type.TokenBalanceTypeRegistry
import io.paritytech.polkadotapp.feature_balances_api.domain.model.TokenBalance
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCheckMemberInRingUseCase
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_people_api.domain.useCase.ActivePeopleCollectionUseCase
import io.paritytech.polkadotapp.feature_pgas_api.domain.OnExistingAllocationStrategy
import io.paritytech.polkadotapp.feature_pgas_api.domain.OnExistingAllocationStrategy.IGNORE
import io.paritytech.polkadotapp.feature_pgas_api.domain.OnExistingAllocationStrategy.INCREASE
import io.paritytech.polkadotapp.feature_pgas_api.domain.PgasChainAssetProvider
import io.paritytech.polkadotapp.feature_pgas_api.domain.PgasClaimer
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class DataStorePgasProvisionerTest {
    private var balance: Balance = Balance.ZERO
    private var topUp: Balance = Balance.ZERO

    private val collections = mockk<ActivePeopleCollectionUseCase>()
    private val ring = mockk<PeopleCheckMemberInRingUseCase>()
    private val claims = mutableListOf<Pair<AccountId, OnExistingAllocationStrategy>>()
    private var claimNeverReportsBack = false

    // mockk resolves a Result return through kotlin-reflect, which rejects context-parameter functions.
    private val claimer = object : PgasClaimer {
        context(diagnostics: StalenessReportCollector)
        override suspend fun claim(destinationAccountId: AccountId, strategy: OnExistingAllocationStrategy): Result<Unit> {
            claims += destinationAccountId to strategy
            if (claimNeverReportsBack) awaitCancellation()
            balance += topUp
            return Result.success(Unit)
        }
    }
    private val registry = mockk<TokenBalanceTypeRegistry>()

    private val provisioner = DataStorePgasProvisioner(
        activePeopleCollectionUseCase = collections,
        peopleCheckMemberInRingUseCase = ring,
        pgasClaimer = claimer,
        pgasChainAssetProvider = mockk<PgasChainAssetProvider> { coEvery { asset() } returns mockk() },
        tokenBalanceTypeRegistry = registry,
    )

    @Before
    fun setUp() {
        coEvery { collections.getActivePeopleCollection() } returns PeopleCollection.LitePeople
        coEvery { ring.awaitIncluded(PeopleCollection.LitePeople) } returns Result.success(Unit)
        every { registry.typeFor(any()) } returns mockk {
            coEvery { getBalance(ACCOUNT) } answers { mockk<TokenBalance> { every { transferable } returns balance } }
        }
    }

    @Test
    fun `an empty account is funded with a claim that keeps any existing allocation`() = runTest {
        topUp = CLAIM_AMOUNT

        val result = provisioner.ensureFunded(ACCOUNT)

        assertTrue(result.isSuccess)
        assertEquals(listOf(ACCOUNT to IGNORE), claims)
    }

    @Test
    fun `a funded account neither claims nor waits on the ring`() = runTest {
        balance = CLAIM_AMOUNT

        val result = provisioner.ensureFunded(ACCOUNT)

        assertTrue(result.isSuccess)
        coVerify(exactly = 0) { ring.awaitIncluded(any()) }
        assertTrue(claims.isEmpty())
    }

    @Test
    fun `a balance covering the requirement claims nothing`() = runTest {
        balance = REQUIRED

        val result = provisioner.ensureCovers(ACCOUNT, REQUIRED)

        assertTrue(result.isSuccess)
        assertTrue(claims.isEmpty())
    }

    @Test
    fun `a short balance is topped up exactly once`() = runTest {
        balance = REQUIRED - 1.intoBalance()
        topUp = CLAIM_AMOUNT

        val result = provisioner.ensureCovers(ACCOUNT, REQUIRED)

        assertTrue(result.isSuccess)
        assertEquals(listOf(ACCOUNT to INCREASE), claims)
    }

    @Test
    fun `a top-up that still leaves the balance short fails the attempt`() = runTest {
        balance = 1.intoBalance()
        topUp = 1.intoBalance()

        val result = provisioner.ensureCovers(ACCOUNT, REQUIRED)

        assertTrue("expected a failure, the account still holds $balance", result.exceptionOrNull() is DataStorePgasShortError)
    }

    @Test
    fun `a claim waits for ring inclusion and fails when it cannot be established`() = runTest {
        coEvery { ring.awaitIncluded(PeopleCollection.LitePeople) } returns Result.failure(IllegalStateException("not in the ring yet"))

        val result = provisioner.ensureCovers(ACCOUNT, REQUIRED)

        assertTrue(result.isFailure)
        assertTrue(claims.isEmpty())
    }

    @Test
    fun `a claim that never reports back fails the attempt instead of holding it forever`() = runTest {
        claimNeverReportsBack = true

        val result = async { provisioner.ensureCovers(ACCOUNT, REQUIRED) }
        advanceTimeBy(CLAIM_TIMEOUT_MS + 1)

        assertTrue("the attempt is still waiting on the claim", result.isCompleted)
        assertTrue(result.await().isFailure)
    }

    private companion object {
        const val CLAIM_TIMEOUT_MS = 2 * 60_000L

        val ACCOUNT = ByteArray(32) { 0x0a }.intoAccountId()
        val REQUIRED: Balance = 500_000_000.intoBalance()
        val CLAIM_AMOUNT: Balance = 10_000_000_000L.intoBalance()
    }
}
