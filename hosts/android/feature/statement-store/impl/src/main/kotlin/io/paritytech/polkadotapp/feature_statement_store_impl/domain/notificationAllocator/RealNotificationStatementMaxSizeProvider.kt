package io.paritytech.polkadotapp.feature_statement_store_impl.domain.notificationAllocator

import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.feature_statement_store_api.domain.notificationAllocator.NotificationStatementMaxSizeProvider
import io.paritytech.polkadotapp.feature_statement_store_impl.data.repository.NotificationSlotRepository
import timber.log.Timber
import javax.inject.Inject

private val RUNTIME_DEFAULT_NOTIFICATION_STATEMENT_MAX_SIZE: InformationSize = 2560.bytes

class RealNotificationStatementMaxSizeProvider @Inject constructor(
    private val notificationSlotRepository: NotificationSlotRepository,
) : NotificationStatementMaxSizeProvider {
    override suspend fun maxSize(): Result<InformationSize> {
        return notificationSlotRepository.notificationStatementMaxSize().map { maxSize ->
            maxSize ?: RUNTIME_DEFAULT_NOTIFICATION_STATEMENT_MAX_SIZE.also {
                Timber.i("notificationStatementMaxSize: NotificationAllowance is not set on chain, using the runtime default ${it.inWholeBytes} bytes")
            }
        }
    }
}
