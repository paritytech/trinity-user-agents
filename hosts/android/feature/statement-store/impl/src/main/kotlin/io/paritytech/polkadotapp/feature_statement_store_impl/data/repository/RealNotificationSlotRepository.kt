package io.paritytech.polkadotapp.feature_statement_store_impl.data.repository

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchAlias
import io.paritytech.polkadotapp.chains.call.MultiChainViewFunctionsApi
import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.getSocket
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.network.rpc.BulkRetriever
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.common.data.memory.SingleValueCache
import io.paritytech.polkadotapp.common.data.memory.getCatching
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_statement_store_impl.data.blockchain.getLiteNotificationSlotsPerPeriod
import io.paritytech.polkadotapp.feature_statement_store_impl.data.blockchain.getNotificationSlotsPerPeriod
import io.paritytech.polkadotapp.feature_statement_store_impl.data.blockchain.notificationRegistrationByAlias
import io.paritytech.polkadotapp.feature_statement_store_impl.data.blockchain.statementStoreResources
import javax.inject.Inject

private val STATEMENT_ALLOWANCE_KEY_PREFIX = ":statement_allowance:".encodeToByteArray()

class RealNotificationSlotRepository @Inject constructor(
    @RemoteSourceQualifier private val storageDataSource: StorageDataSource,
    private val viewFunctionsApi: MultiChainViewFunctionsApi,
    private val knownChains: KnownChains,
    private val chainRegistry: ChainRegistry,
    private val bulkRetriever: BulkRetriever,
) : NotificationSlotRepository {
    // Unwrapping inside the compute keeps a failed read out of the cache; getCatching re-wraps the throw.
    private val highestSeqCache = SingleValueCache {
        peopleViewFunctions().getNotificationSlotsPerPeriod().getOrThrow()
    }

    private val liteHighestSeqCache = SingleValueCache {
        peopleViewFunctions().getLiteNotificationSlotsPerPeriod().getOrThrow()
    }

    override suspend fun highestSeqPerPeriod(collection: PeopleCollection): Result<UByte> = when (collection) {
        PeopleCollection.People -> highestSeqCache.getCatching()
        PeopleCollection.LitePeople -> liteHighestSeqCache.getCatching()
    }

    override suspend fun registeredAliases(
        chainId: ChainId,
        candidates: Collection<BandersnatchAlias>,
    ): Result<Set<BandersnatchAlias>> = runCancellableCatching {
        if (candidates.isEmpty()) return@runCancellableCatching emptySet()

        storageDataSource.query(chainId) {
            runtime.metadata.statementStoreResources.notificationRegistrationByAlias.entries(candidates).keys
        }
    }

    override suspend fun hasStatementAllowance(
        chainId: ChainId,
        accounts: Collection<AccountId>,
        at: BlockHash?,
    ): Result<Map<AccountId, Boolean>> = runCancellableCatching {
        val keysByAccount = accounts.associateWith(::statementAllowanceKey)
        val values = bulkRetriever.queryKeys(chainRegistry.getSocket(chainId), keysByAccount.values.toList(), at)
        val answered = values.mapKeys { (key, _) -> key.lowercase() }

        keysByAccount.mapNotNull { (account, key) ->
            if (key !in answered) return@mapNotNull null
            account to (answered[key] != null)
        }.toMap()
    }

    private fun statementAllowanceKey(account: AccountId): String =
        (STATEMENT_ALLOWANCE_KEY_PREFIX + account.value).toHexString(withPrefix = true).lowercase()

    private suspend fun peopleViewFunctions() = viewFunctionsApi.forChain(knownChains.people)
}
