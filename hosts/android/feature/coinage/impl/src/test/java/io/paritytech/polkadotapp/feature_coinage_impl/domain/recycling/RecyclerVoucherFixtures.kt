package io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import org.mockito.Mockito.mock

const val FULL_RING_CAPACITY = 767

fun voucherOf(location: RecyclerVoucher.Location, ringVrfKeyIndex: Int = 0) = RecyclerVoucher(
    ringVrfKeyIndex = testKey(ringVrfKeyIndex),
    ringVrfPublicKey = mock(),
    recyclerValue = ValueExponent(1),
    location = location,
    recyclerFungibility = RecyclerFungibility.NONE,
    maxRecyclerFungibility = RecyclerFungibility.NONE,
)
