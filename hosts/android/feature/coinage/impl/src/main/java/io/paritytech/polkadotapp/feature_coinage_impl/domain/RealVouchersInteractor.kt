package io.paritytech.polkadotapp.feature_coinage_impl.domain

import io.paritytech.polkadotapp.feature_coinage_api.domain.RecyclerVouchersInteractor
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.VoucherRepository
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

class RealVouchersInteractor @Inject constructor(
    private val voucherRepository: VoucherRepository
) : RecyclerVouchersInteractor {
    override fun subscribeVouchers(): Flow<List<RecyclerVoucher>> {
        return voucherRepository.subscribeAllVouchers()
    }
}
