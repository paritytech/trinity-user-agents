package io.paritytech.polkadotapp.feature_dotns_gateway_api.data.repository

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.EncodedPublicKey
import io.paritytech.polkadotapp.feature_dotns_gateway_api.domain.model.DotNsBaseNameAvailability
import io.paritytech.polkadotapp.feature_dotns_gateway_api.domain.model.DotNsLink
import kotlinx.coroutines.flow.Flow

interface DotNsGatewayRepository {
    fun observeHasFullUsername(accountId: AccountId): Flow<Boolean>

    suspend fun getBaseNameAvailability(baseLabel: String, accountId: AccountId): Result<DotNsBaseNameAvailability>

    suspend fun registerName(who: AccountId, label: String, link: DotNsLink): Result<Unit>

    fun reservationSigningPayload(
        candidate: AccountId,
        attester: AccountId,
        usernameBase: String,
        chatKey: EncodedPublicKey,
        reservedBaseLabel: String?,
        signedAt: Long
    ): ByteArray
}
