package io.paritytech.polkadotapp.feature_statement_store_api.domain.notificationAllocator

import io.paritytech.polkadotapp.common.utils.InformationSize

interface NotificationStatementMaxSizeProvider {
    suspend fun maxSize(): Result<InformationSize>
}
