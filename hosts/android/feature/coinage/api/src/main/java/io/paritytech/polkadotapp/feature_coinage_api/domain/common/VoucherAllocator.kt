package io.paritytech.polkadotapp.feature_coinage_api.domain.common

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent

interface VoucherAllocator {
    suspend fun allocate(valueExponent: ValueExponent): Result<RecyclerVoucher>

    suspend fun allocateAll(valueExponents: List<ValueExponent>): Result<List<RecyclerVoucher>>
}
