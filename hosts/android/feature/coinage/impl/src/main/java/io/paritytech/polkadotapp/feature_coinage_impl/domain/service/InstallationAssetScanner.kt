package io.paritytech.polkadotapp.feature_coinage_impl.domain.service

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchPublicKey
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.filterNotNull
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.measureExecution
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin.Age
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher.Location
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.CoinKeypairDerivation
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.VoucherRingDerivation
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.getDerivedAccountIds
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.getDerivedMemberKeys
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainAliasState
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainCoinInfo
import io.paritytech.polkadotapp.feature_coinage_impl.data.signer.context.CoinageSigningContextProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.CoinageStateReader
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.CoinageStateReaderFactory
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.RecyclerAliasKey
import io.paritytech.polkadotapp.feature_members_api.data.model.RingPosition
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.PinnedChainViewFactory
import javax.inject.Inject

interface InstallationAssetScanner {
    suspend fun scanCoins(installation: CoinageInstallationId, startIndex: Int, count: Int): Result<List<Coin>>

    suspend fun scanVouchers(installation: CoinageInstallationId, startIndex: Int, count: Int): Result<List<RecyclerVoucher>>
}

class RealInstallationAssetScanner @Inject constructor(
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
    private val chainViewFactory: PinnedChainViewFactory,
    private val stateReaderFactory: CoinageStateReaderFactory,
    private val keypairDerivation: CoinKeypairDerivation,
    private val voucherRingDerivation: VoucherRingDerivation,
    private val coinageSigningContextProvider: CoinageSigningContextProvider,
) : InstallationAssetScanner {
    override suspend fun scanCoins(installation: CoinageInstallationId, startIndex: Int, count: Int): Result<List<Coin>> {
        val indices = indicesOf(installation, startIndex, count)
        val accounts = measureExecution("deriving accounts for coins") {
            keypairDerivation.getDerivedAccountIds(indices).zip(indices).toMap()
        }

        return finalizedState().flatMap { state ->
            measureExecution("fetching on chain data for coins batch") {
                state.reader.coinsAt(state.at, accounts.keys.toList())
            }
        }
            .map { it.filterNotNull() }
            .map { it.toCoins(accounts) }
    }

    override suspend fun scanVouchers(
        installation: CoinageInstallationId,
        startIndex: Int,
        count: Int
    ): Result<List<RecyclerVoucher>> {
        val indices = indicesOf(installation, startIndex, count)
        val keys = measureExecution("Deriving accounts for vouchers") {
            voucherRingDerivation.getDerivedMemberKeys(indices).zip(indices).toMap()
        }

        return finalizedState().flatMap { state ->
            fetchVouchersOnChainData(state, keys.keys.toList())
                .flatMap { (values, records) ->
                    val detected = records.toVouchers(keys, values)

                    fetchNotUnloadedVouchers(state, detected.filter { it.location is Location.InRecycler })
                        .map { inRecycler -> inRecycler + detected.filter { it.location is Location.Onboarding } }
                }
        }
    }

    /**
     * Recovery reads the finalized chain, never the best head. An asset saved from a best-head read can
     * outlive the block that justified it, and the row left behind is one nothing local ever minted: no
     * ledger entry explains it, so a payment made of it can never be told apart from one still in flight.
     */
    private suspend fun finalizedState(): Result<FinalizedState> {
        return chainViewFactory.pin(chainAssetProvider.chainId()).mapCatching { view ->
            FinalizedState(reader = stateReaderFactory.create(view), at = view.finalizedHead.blockHash)
        }
    }

    private class FinalizedState(val reader: CoinageStateReader, val at: BlockHash)

    private fun indicesOf(installation: CoinageInstallationId, startIndex: Int, count: Int) =
        (startIndex until startIndex + count).map { CoinageKeyIndex(installation, it) }

    private fun Map<AccountId, OnChainCoinInfo>.toCoins(accounts: Map<AccountId, CoinageKeyIndex>) = mapNotNull { (accountId, onChainInfo) ->
        Coin(
            derivationIndex = accounts[accountId] ?: return@mapNotNull null,
            valueExponent = ValueExponent(onChainInfo.value),
            accountId = accountId,
            // Recovered from a read that found it, so it is on chain by construction.
            age = Age.Known(onChainInfo.age),
            isOnChain = true,
            // Recovery reads value and age, never where the coin has been. Left unobserved so the presence
            // sync fills the history in from the age, the same way it does for a claimed coin.
            provenance = CoinProvenance.UNKNOWN
        )
    }

    private fun Map<BandersnatchPublicKey, RingPosition>.toVouchers(
        keys: Map<BandersnatchPublicKey, CoinageKeyIndex>,
        values: Map<BandersnatchPublicKey, ValueExponent>
    ) = mapNotNull { (publicKey, onChainInfo) ->
        RecyclerVoucher(
            ringVrfKeyIndex = keys[publicKey] ?: return@mapNotNull null,
            ringVrfPublicKey = publicKey,
            recyclerValue = values[publicKey] ?: return@mapNotNull null,
            location = onChainInfo.getVoucherLocation(),
            // Recovery knows where the voucher sits, not how drained its ring is. Zero until the location
            // service reads it; the max is left unfrozen so that service writes a real one rather than
            // inheriting this placeholder.
            recyclerFungibility = RecyclerFungibility.NONE,
            maxRecyclerFungibility = null,
        )
    }

    private suspend fun fetchNotUnloadedVouchers(state: FinalizedState, detected: List<RecyclerVoucher>): Result<List<RecyclerVoucher>> {
        if (detected.isEmpty()) return Result.success(listOf())

        val aliasContext = coinageSigningContextProvider.recyclerVouchersContext()
        val keys = detected.mapNotNull { voucher ->
            val location = (voucher.location as? Location.InRecycler) ?: return@mapNotNull null
            val alias = voucherRingDerivation.aliasOf(voucher.ringVrfKeyIndex, aliasContext)

            voucher to RecyclerAliasKey(
                valueExponent = voucher.recyclerValue.value.toBigInteger(),
                recyclerIndex = location.recyclerIndex.value,
                alias = alias.value.toDataByteArray(),
            )
        }.toMap()

        return state.reader.aliasStatesAt(state.at, keys.values.toList()).mapCatching { aliasStates ->
            keys.mapNotNull { (voucher, key) -> voucher.takeIf { aliasStates.getValue(key) !is OnChainAliasState.Unloaded } }
        }
    }

    private suspend fun fetchVouchersOnChainData(
        state: FinalizedState,
        memberKeys: List<BandersnatchPublicKey>
    ) = measureExecution("Fetching vouchers on chain info") {
        state.reader.recyclerMembershipsAt(state.at, memberKeys)
            .map { it.filterNotNull() }
            .flatMap { memberships ->
                state.reader.ringPositionsAt(state.at, memberships)
                    .map { positions -> memberships to positions.filterNotNull() }
            }
    }

    private fun RingPosition.getVoucherLocation() = when (this) {
        // Recovery knows where the voucher sits, not how full the ring is. Zero until the location service
        // reads it, so nothing releases the voucher on an anonymity set we have not seen.
        is RingPosition.Included -> Location.InRecycler(ringIndex, recyclerMembers = 0, enteredAt = null)
        is RingPosition.Onboarding -> Location.Onboarding
        is RingPosition.Suspended -> Location.Unknown
    }
}
