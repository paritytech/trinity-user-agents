package io.paritytech.polkadotapp.feature_coinage_impl.domain.model

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.CoinAllocator
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.VoucherAllocator
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageInput
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import javax.inject.Inject

class CoinageTransactionFactory @Inject constructor(
    private val coinAllocator: CoinAllocator,
    private val voucherAllocator: VoucherAllocator,
) : CoinageTransaction.Factory {
    override fun newTransaction(): CoinageTransaction = RealCoinageTransaction(coinAllocator, voucherAllocator)
}

private class RealCoinageTransaction(
    private val coinAllocator: CoinAllocator,
    private val voucherAllocator: VoucherAllocator,
) : CoinageTransaction {
    private val inputs = mutableListOf<CoinageInput>()
    private val outputs = mutableListOf<OwnAsset>()
    private val handedOff = mutableListOf<OwnAsset>()

    override suspend fun mintCoins(
        valueExponents: List<ValueExponent>,
        provenance: CoinProvenance
    ): Result<List<Coin>> =
        coinAllocator.allocateAll(valueExponents, provenance).onSuccess { coins ->
            outputs += coins.map { OwnAsset.Coin(it.derivationIndex) }
        }

    override suspend fun mintVoucher(valueExponent: ValueExponent): Result<RecyclerVoucher> =
        voucherAllocator.allocate(valueExponent).onSuccess { voucher ->
            outputs += OwnAsset.Voucher(voucher.ringVrfKeyIndex)
        }

    override fun consumeCoins(coins: List<Coin>) {
        inputs += coins.map { CoinageInput.Coin.Own(it.derivationIndex) }
    }

    override fun consumeReceivedCoin(publicKey: DataByteArray) {
        inputs += CoinageInput.Coin.Received(publicKey)
    }

    override fun useVouchers(vouchers: List<RecyclerVoucher>) {
        inputs += vouchers.map { CoinageInput.Voucher(it.ringVrfKeyIndex) }
    }

    override fun handOff(assets: List<OwnAsset>) {
        handedOff += assets
    }

    override fun build() = CoinageTransactionAssets(inputs.toList(), outputs.toList(), handedOff.toList())
}
