package io.paritytech.polkadotapp.feature_coinage_api.domain

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import kotlinx.coroutines.flow.Flow

interface RecyclerVouchersInteractor {
    fun subscribeVouchers(): Flow<List<RecyclerVoucher>>
}
