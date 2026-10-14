package io.paritytech.polkadotapp.feature_transactions.api.domain.durable

import dagger.MapKey

/** Keys an [AsyncDurableSubmissionPolicy] into the engine's map of them by [SubmissionPolicy.id]. */
@MapKey
annotation class SubmissionPolicyKey(val value: String)
