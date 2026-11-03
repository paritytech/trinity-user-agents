package io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator

/** Names the consumer that allocated a slot, so it can find exactly its own slots later. */
@JvmInline
value class StatementStoreSlotTag(val value: String)
