package io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator

import io.paritytech.polkadotapp.feature_transactions.api.data.validation.InvalidTransaction

class EvictionNotYetAllowedException(message: String) : Exception(message)

class SlotReplacementRejectedException(val reason: InvalidTransaction) : Exception("Slot replacement rejected by the runtime: $reason")
