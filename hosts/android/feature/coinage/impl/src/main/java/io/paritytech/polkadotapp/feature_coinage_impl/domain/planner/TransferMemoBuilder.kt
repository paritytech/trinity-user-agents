package io.paritytech.polkadotapp.feature_coinage_impl.domain.planner

import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.formatExponentsToBalance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.PlannedMemoEntry
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferCoinEntry
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferMemo
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageBalanceConverterUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.CoinKeypairDerivation
import javax.inject.Inject

class TransferMemoBuilder @Inject constructor(
    private val coinKeypairDerivation: CoinKeypairDerivation,
    private val coinageBalanceConverterUseCase: CoinageBalanceConverterUseCase
) {
    suspend fun buildMemo(memoEntries: List<PlannedMemoEntry>): Result<TransferMemo> {
        val coinEntries = memoEntries.map {
            val keyPair = coinKeypairDerivation.deriveKeypair(it.coinDerivationIndex) as Sr25519Keypair

            TransferCoinEntry(
                privateKey = (keyPair.privateKey + keyPair.nonce).toDataByteArray(),
                valueExponent = it.valueExponent
            )
        }

        return coinageBalanceConverterUseCase.create()
            .map { convertionContext ->
                val memoExponents = coinEntries.map { it.valueExponent }
                TransferMemo(
                    coins = coinEntries,
                    totalValue = convertionContext.formatExponentsToBalance(memoExponents)
                )
            }
    }
}
