package io.paritytech.polkadotapp.feature_account_impl.presentation.address.mixin

import io.paritytech.polkadotapp.common.presentation.search.RemoteSearchPhase
import io.paritytech.polkadotapp.common.presentation.search.RemoteSearchSession
import io.paritytech.polkadotapp.common.presentation.ui.mixin.paste.PasteMixin
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.common.utils.shareInBackground
import io.paritytech.polkadotapp.feature_account_api.presentation.address.mixin.AddressInputMixin
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.AddressCandidates
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddress
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddressesSection
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map

internal class RealAddressInputMixin(
    private val pasteMixinFactory: PasteMixin.Factory,
    private val localConverters: List<AddressInputMixin.AddressConverter>,
    private val remoteConverters: List<AddressInputMixin.AddressConverter>,
    private val coroutineScope: CoroutineScope,
) : AddressInputMixin,
    CoroutineScope by coroutineScope {
    override val paste: PasteMixin = pasteMixinFactory.create {
        input.value = it
    }

    override val input: MutableStateFlow<String> = MutableStateFlow("")

    private val remoteSearch = RemoteSearchSession(
        search = { query -> Result.success(remoteConverters.convert(query).flatMap { it.addresses }) },
        matchesQuery = { address, query ->
            address.type == ExtractedAddress.DisplayType.USERNAME && address.display.startsWith(query, ignoreCase = true)
        },
    )

    @OptIn(ExperimentalCoroutinesApi::class)
    override val addressCandidates = input
        .flatMapLatest { query ->
            val local = localConverters.convert(query)

            if (query.isEmpty()) {
                flowOf(AddressCandidates(query, local, RemoteSearchPhase.Loaded(emptyList())))
            } else {
                remoteSearch.phases(query).map { remote -> AddressCandidates(query, local, remote) }
            }
        }
        .shareInBackground()

    private suspend fun List<AddressInputMixin.AddressConverter>.convert(query: String): List<ExtractedAddressesSection> {
        return mapNotNull { converter ->
            runCancellableCatching { converter.convertToAddress(query) }.getOrNull()
        }
    }
}
