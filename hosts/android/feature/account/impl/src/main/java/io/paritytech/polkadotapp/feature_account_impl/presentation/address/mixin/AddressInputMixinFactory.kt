package io.paritytech.polkadotapp.feature_account_impl.presentation.address.mixin

import io.paritytech.polkadotapp.common.presentation.ui.mixin.paste.PasteMixin
import io.paritytech.polkadotapp.feature_account_api.presentation.address.mixin.AddressInputMixin
import kotlinx.coroutines.CoroutineScope
import javax.inject.Inject

class AddressInputMixinFactory @Inject constructor(
    private val pasteMixinFactory: PasteMixin.Factory,
) : AddressInputMixin.Factory {
    override fun create(
        coroutineScope: CoroutineScope,
        localConverters: List<AddressInputMixin.AddressConverter>,
        remoteConverters: List<AddressInputMixin.AddressConverter>,
    ): AddressInputMixin {
        return RealAddressInputMixin(
            pasteMixinFactory = pasteMixinFactory,
            localConverters = localConverters,
            remoteConverters = remoteConverters,
            coroutineScope = coroutineScope
        )
    }
}
