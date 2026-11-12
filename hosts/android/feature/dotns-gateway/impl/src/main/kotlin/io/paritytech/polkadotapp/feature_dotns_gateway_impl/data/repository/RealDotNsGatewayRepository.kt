package io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.repository

import io.paritytech.polkadotapp.chains.di.LocalSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.query.metadata
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.EncodedPublicKey
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_gateway_api.data.api.accountAlias
import io.paritytech.polkadotapp.feature_dotns_gateway_api.data.api.dotNsGateway
import io.paritytech.polkadotapp.feature_dotns_gateway_api.data.repository.DotNsGatewayRepository
import io.paritytech.polkadotapp.feature_dotns_gateway_api.domain.model.DotNsBaseNameAvailability
import io.paritytech.polkadotapp.feature_dotns_gateway_api.domain.model.DotNsLink
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.DotNsReservationMessage
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.api.dotNsGatewayCalls
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.api.registerName
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.config.DotNsGatewayConfig
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.config.DotNsGatewayConfigProvider
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.contract.PopContractCoder
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.signer.origins.DotNsGatewayOrigins
import io.paritytech.polkadotapp.feature_revive_api.NameHash
import io.paritytech.polkadotapp.feature_revive_api.ReviveContractApi
import io.paritytech.polkadotapp.feature_revive_api.toEvmAccountId
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.data.flattenExecutionFailure
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

class RealDotNsGatewayRepository @Inject constructor(
    @param:LocalSourceQualifier private val localStorageSource: StorageDataSource,
    private val reviveContractApi: ReviveContractApi,
    private val configProvider: DotNsGatewayConfigProvider,
    private val tldProvider: DotNsTldProvider,
    private val extrinsicService: ExtrinsicService,
    private val dotNsGatewayOrigins: DotNsGatewayOrigins,
    private val chainRegistry: ChainRegistry,
    private val knownChains: KnownChains
) : DotNsGatewayRepository {
    override fun observeHasFullUsername(accountId: AccountId): Flow<Boolean> {
        return localStorageSource.subscribe(knownChains.assetHub) {
            metadata.dotNsGateway.accountAlias.observe(accountId)
                .map { alias -> alias != null }
        }
    }

    override suspend fun getBaseNameAvailability(
        baseLabel: String,
        accountId: AccountId
    ): Result<DotNsBaseNameAvailability> {
        return configProvider.getConfig().flatMap { config ->
            isRegistered(baseLabel, config).flatMap { registered ->
                if (registered) {
                    Result.success(DotNsBaseNameAvailability.TakenByOther)
                } else {
                    reservationAvailability(baseLabel, accountId, config)
                }
            }
        }
    }

    override fun reservationSigningPayload(
        candidate: AccountId,
        attester: AccountId,
        usernameBase: String,
        chatKey: EncodedPublicKey,
        reservedBaseLabel: String?,
        signedAt: Long
    ): ByteArray {
        return DotNsReservationMessage.signingPayload(
            candidate = candidate,
            attester = attester,
            usernameBase = usernameBase,
            chatKey = chatKey,
            reservedBaseLabel = reservedBaseLabel,
            signedAt = signedAt
        )
    }

    override suspend fun registerName(who: AccountId, label: String, link: DotNsLink): Result<Unit> {
        return extrinsicService.submitExtrinsicAndAwaitExecution(
            chain = chainRegistry.getChain(knownChains.assetHub),
            origin = dotNsGatewayOrigins.asPersonRegistration(who, label, link)
        ) {
            dotNsGatewayCalls.registerName(
                who = who,
                label = label,
                link = link
            )
        }
            .flattenExecutionFailure()
            .coerceToUnit()
    }

    private suspend fun isRegistered(baseLabel: String, config: DotNsGatewayConfig): Result<Boolean> {
        return tldProvider.getTld().flatMap { tld ->
            val input = PopContractCoder.encodeChatKey(NameHash.nodeUnderTld(tld.value, baseLabel))

            reviveContractApi.callReadOnly(
                chainId = knownChains.assetHub,
                contract = config.popResolverAddress,
                input = input.toDataByteArray()
            ).map { output -> PopContractCoder.decodeChatKey(output.value) != null }
        }
    }

    private suspend fun reservationAvailability(
        baseLabel: String,
        accountId: AccountId,
        config: DotNsGatewayConfig
    ): Result<DotNsBaseNameAvailability> {
        val input = PopContractCoder.encodeIsReservedForClaim(baseLabel)

        return reviveContractApi.callReadOnly(
            chainId = knownChains.assetHub,
            contract = config.popControllerAddress,
            input = input.toDataByteArray()
        ).mapCatching { output ->
            val reservation = PopContractCoder.decodeIsReservedForClaim(output.value)
                ?: error("Malformed isReservedForClaim output for $baseLabel")

            when {
                !reservation.reserved -> DotNsBaseNameAvailability.Free
                reservation.holder.toDataByteArray() == accountId.toEvmAccountId() -> DotNsBaseNameAvailability.ReservedByUs
                else -> DotNsBaseNameAvailability.TakenByOther
            }
        }
    }
}
