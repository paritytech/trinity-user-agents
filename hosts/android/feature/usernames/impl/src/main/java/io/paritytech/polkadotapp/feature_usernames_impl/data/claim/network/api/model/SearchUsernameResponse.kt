package io.paritytech.polkadotapp.feature_usernames_impl.data.claim.network.api.model

import androidx.annotation.Keep
import com.google.gson.annotations.SerializedName
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.util.accountIdOf
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.FoundUser
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.OnChainData
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username

@Keep
data class SearchUsernameBaseResponse(
    val usernames: List<SearchUsernameResponse>,
)

@Keep
data class SearchUsernameResponse(
    val accountId: String?,
    val username: String,
    val status: String,
    @SerializedName("onchainData")
    val onChainData: OnChainDataResponse?,
    val createdAt: String,
    val updatedAt: String,
)

@Keep
data class OnChainDataResponse(
    val blockHash: String,
    val blockNumber: Long,
    val blockIndex: Long,
    val eventIndex: Long,
)

fun SearchUsernameResponse.toDomain(chain: Chain): FoundUser? {
    return FoundUser(
        accountId = chain.accountIdOf(accountId ?: return null),
        username = Username.fromFullValue(username),
        onChainData = onChainData?.toDomain(),
    )
}

fun OnChainDataResponse.toDomain(): OnChainData = OnChainData(
    blockHash = blockHash,
    blockNumber = blockNumber,
    blockIndex = blockIndex,
    eventIndex = eventIndex,
)
