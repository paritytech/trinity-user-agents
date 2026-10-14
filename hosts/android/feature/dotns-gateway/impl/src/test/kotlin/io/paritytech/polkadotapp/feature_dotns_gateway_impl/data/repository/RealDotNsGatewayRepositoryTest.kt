@file:OptIn(ExperimentalStdlibApi::class)

package io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.repository

import io.mockk.coEvery
import io.mockk.mockk
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_gateway_api.domain.model.DotNsBaseNameAvailability
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.config.DotNsGatewayConfig
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.config.DotNsGatewayConfigProvider
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.signer.origins.DotNsGatewayOrigins
import io.paritytech.polkadotapp.feature_revive_api.ReviveContractApi
import io.paritytech.polkadotapp.feature_revive_api.toEvmAccountId
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.web3j.abi.FunctionEncoder
import org.web3j.abi.TypeReference
import org.web3j.abi.datatypes.Address
import org.web3j.abi.datatypes.Bool
import org.web3j.abi.datatypes.DynamicBytes
import org.web3j.abi.datatypes.Function
import org.web3j.abi.datatypes.Type

class RealDotNsGatewayRepositoryTest {
    private val knownChains = KnownChains(people = "", assetHub = CHAIN_ID, bulletIn = "", hydration = null)
    private val localStorageSource: StorageDataSource = mockk()
    private val reviveContractApi: ReviveContractApi = mockk()
    private val configProvider: DotNsGatewayConfigProvider = mockk()
    private val tldProvider: DotNsTldProvider = mockk()
    private val extrinsicService: ExtrinsicService = mockk()
    private val dotNsGatewayOrigins: DotNsGatewayOrigins = mockk()
    private val chainRegistry: ChainRegistry = mockk()

    private val repository = RealDotNsGatewayRepository(
        localStorageSource = localStorageSource,
        reviveContractApi = reviveContractApi,
        configProvider = configProvider,
        tldProvider = tldProvider,
        extrinsicService = extrinsicService,
        dotNsGatewayOrigins = dotNsGatewayOrigins,
        chainRegistry = chainRegistry,
        knownChains = knownChains
    )

    @Test
    fun `TakenByOther when the name is already registered`() = runBlocking<Unit> {
        stubConfig()
        stubChatKey(ByteArray(65) { 7 })

        assertEquals(DotNsBaseNameAvailability.TakenByOther, repository.getBaseNameAvailability(NAME, OUR_ACCOUNT_ID).getOrThrow())
    }

    @Test
    fun `Free when not registered and the queue has no live head`() = runBlocking<Unit> {
        stubConfig()
        stubChatKey(ByteArray(0))
        stubReservation(reserved = false, holder = ByteArray(20))

        assertEquals(DotNsBaseNameAvailability.Free, repository.getBaseNameAvailability(NAME, OUR_ACCOUNT_ID).getOrThrow())
    }

    @Test
    fun `ReservedByUs when our reservation heads the queue`() = runBlocking<Unit> {
        stubConfig()
        stubChatKey(ByteArray(0))
        stubReservation(reserved = true, holder = OUR_ACCOUNT_ID.toEvmAccountId().value)

        assertEquals(DotNsBaseNameAvailability.ReservedByUs, repository.getBaseNameAvailability(NAME, OUR_ACCOUNT_ID).getOrThrow())
    }

    @Test
    fun `TakenByOther when someone else holds the live reservation`() = runBlocking<Unit> {
        stubConfig()
        stubChatKey(ByteArray(0))
        stubReservation(reserved = true, holder = ByteArray(20) { 9 })

        assertEquals(DotNsBaseNameAvailability.TakenByOther, repository.getBaseNameAvailability(NAME, OUR_ACCOUNT_ID).getOrThrow())
    }

    private fun stubConfig() = runBlocking {
        coEvery { configProvider.getConfig() } returns
            Result.success(DotNsGatewayConfig(popControllerAddress = CONTROLLER, popResolverAddress = RESOLVER))
        coEvery { tldProvider.getTld() } returns Result.success(requireNotNull(DotNsTld.parse("dot")))
    }

    private fun stubChatKey(key: ByteArray) = runBlocking {
        val output = abiEncodeReturnValues(
            listOf(DynamicBytes(key)),
            listOf(object : TypeReference<DynamicBytes>() {})
        )
        coEvery { reviveContractApi.callReadOnly(CHAIN_ID, RESOLVER, any(), isNull()) } returns Result.success(output.toDataByteArray())
    }

    private fun stubReservation(reserved: Boolean, holder: ByteArray) = runBlocking {
        val output = abiEncodeReturnValues(
            listOf(Bool(reserved), Address("0x" + holder.toHexString())),
            listOf(object : TypeReference<Bool>() {}, object : TypeReference<Address>() {})
        )
        coEvery { reviveContractApi.callReadOnly(CHAIN_ID, CONTROLLER, any(), isNull()) } returns Result.success(output.toDataByteArray())
    }

    @Suppress("UNCHECKED_CAST")
    private fun abiEncodeReturnValues(values: List<Type<*>>, typeRefs: List<TypeReference<out Type<*>>>): ByteArray {
        val function = Function("_", values, typeRefs as List<TypeReference<Type<*>>>)
        val encoded = FunctionEncoder.encode(function).fromHex()
        return encoded.copyOfRange(4, encoded.size)
    }

    private companion object {
        const val CHAIN_ID = "asset-hub-chain-id"
        const val NAME = "byteboro"
        val OUR_ACCOUNT_ID: AccountId = ByteArray(32) { 1 }.toDataByteArray()
        val CONTROLLER: DataByteArray = ByteArray(20) { 4 }.toDataByteArray()
        val RESOLVER: DataByteArray = ByteArray(20) { 5 }.toDataByteArray()
    }
}
