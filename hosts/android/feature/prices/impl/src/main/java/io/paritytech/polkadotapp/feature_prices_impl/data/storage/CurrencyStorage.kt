package io.paritytech.polkadotapp.feature_prices_impl.data.storage

import io.paritytech.polkadotapp.common.data.storage.SingleValueStorage
import io.paritytech.polkadotapp.common.data.storage.SingleValueStorageFactory
import io.paritytech.polkadotapp.common.data.storage.preferences.store.PreferencesSingleValueSerializer
import io.paritytech.polkadotapp.feature_prices_api.domain.model.Currency
import io.paritytech.polkadotapp.feature_prices_api.domain.model.fromCode

internal typealias CurrencyStorage = SingleValueStorage<Currency>

private const val KEY_SELECTED_CURRENCY = "selected_currency"

internal fun SingleValueStorageFactory.createCurrencyStorage(): CurrencyStorage {
    return preferences(
        key = KEY_SELECTED_CURRENCY,
        default = Currency.USD,
        serializer = PreferencesSingleValueSerializer.from(
            toString = { it.code },
            fromString = { code -> Currency.fromCode(code) }
        )
    )
}
