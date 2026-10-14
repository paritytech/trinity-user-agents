package io.paritytech.polkadotapp.feature_statement_store_api.domain

import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.EncodedMessage

interface MessageCompactor {
    suspend fun compact(messages: List<EncodedMessage>): Result<List<CompactedBatch>>
}

class CompactedBatch(val commit: EncodedMessage, val originals: List<EncodedMessage>)
