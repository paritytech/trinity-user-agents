package io.paritytech.polkadotapp.feature_transactions.api.data

import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin

interface MultiExtrinsicBuilder {
    fun extrinsic(origin: TransactionOrigin, formExtrinsic: FormExtrinsic)
}

class StoringMultiExtrinsicBuilder : MultiExtrinsicBuilder {
    private var extrinsics = mutableListOf<FormExtrinsicWithOrigin>()

    override fun extrinsic(origin: TransactionOrigin, formExtrinsic: FormExtrinsic) {
        extrinsics.add(
            FormExtrinsicWithOrigin(
                formExtrinsic = formExtrinsic,
                origin = origin
            )
        )
    }

    fun build(): List<FormExtrinsicWithOrigin> {
        return extrinsics
    }
}
