package io.paritytech.polkadotapp.feature_upgrade_username_impl.domain.usecase

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_dotns_gateway_api.data.repository.DotNsGatewayRepository
import io.paritytech.polkadotapp.feature_dotns_gateway_api.domain.model.DotNsBaseNameAvailability
import io.paritytech.polkadotapp.feature_upgrade_username_api.domain.model.UpgradeUsernameAvailabilityState
import junit.framework.TestCase.assertEquals
import kotlinx.coroutines.runBlocking
import org.junit.Test

class RealCheckUsernameAvailabilityUseCaseTest {
    private val knownChains = KnownChains(people = "", assetHub = CHAIN_ID, bulletIn = "", hydration = null)
    private val chainRegistry: ChainRegistry = mockk()
    private val accountRepository: AccountRepository = mockk()
    private val dotNsGatewayRepository: DotNsGatewayRepository = mockk()

    private val useCase = RealCheckUsernameAvailabilityUseCase(
        knownChains = knownChains,
        chainRegistry = chainRegistry,
        accountRepository = accountRepository,
        dotNsGatewayRepository = dotNsGatewayRepository
    )

    @Test
    fun `Free when dotNS reports the name available`() = runBlocking<Unit> {
        stubOurAccount()
        stubAvailability(DotNsBaseNameAvailability.Free)

        assertEquals(UpgradeUsernameAvailabilityState.Free, useCase(NAME).getOrThrow())
    }

    @Test
    fun `ReservedByUs when dotNS reports our live reservation`() = runBlocking<Unit> {
        stubOurAccount()
        stubAvailability(DotNsBaseNameAvailability.ReservedByUs)

        assertEquals(UpgradeUsernameAvailabilityState.ReservedByUs, useCase(NAME).getOrThrow())
    }

    @Test
    fun `NotAvailable when dotNS reports the name taken`() = runBlocking<Unit> {
        stubOurAccount()
        stubAvailability(DotNsBaseNameAvailability.TakenByOther)

        assertEquals(UpgradeUsernameAvailabilityState.NotAvailable, useCase(NAME).getOrThrow())
    }

    private fun stubAvailability(availability: DotNsBaseNameAvailability) = runBlocking {
        coEvery { dotNsGatewayRepository.getBaseNameAvailability(NAME, OUR_ACCOUNT_ID) } returns Result.success(availability)
    }

    private fun stubOurAccount() = runBlocking {
        val chain = mockk<Chain>()
        coEvery { chainRegistry.getChain(CHAIN_ID) } returns chain
        val account = mockk<MetaAccount>().also { every { it.accountIdIn(any()) } returns OUR_ACCOUNT_ID }
        coEvery { accountRepository.getWalletAccount() } returns account
    }

    private companion object {
        const val CHAIN_ID = "asset-hub-chain-id"
        const val NAME = "byteboro"
        val OUR_ACCOUNT_ID: AccountId = ByteArray(32) { 1 }.toDataByteArray()
    }
}
