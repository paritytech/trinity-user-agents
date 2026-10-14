package io.paritytech.polkadotapp.feature_transaction_storage_impl.data

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.hash.Hasher.blake2b256
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_transaction_storage_api.data.calls.store
import io.paritytech.polkadotapp.feature_transaction_storage_api.data.calls.transactionStorage
import io.paritytech.polkadotapp.feature_transaction_storage_api.domain.TransactionStorageService
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.data.flattenExecutionFailure
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import javax.inject.Inject

class RealTransactionStorageService @Inject constructor(
    private val chainRegistry: ChainRegistry,
    private val extrinsicService: ExtrinsicService,
) : TransactionStorageService {
    override suspend fun store(data: ByteArray, origin: TransactionOrigin): Result<String> {
        return extrinsicService.submitExtrinsicAndAwaitExecution(chainRegistry.bulletInChain(), origin) {
            transactionStorage.store(data)
        }
            .flattenExecutionFailure()
            .map { data.blake2b256().toHexString(withPrefix = true) }
            .logFailure("Failed to store data (${data.size.bytes})")
    }
}
