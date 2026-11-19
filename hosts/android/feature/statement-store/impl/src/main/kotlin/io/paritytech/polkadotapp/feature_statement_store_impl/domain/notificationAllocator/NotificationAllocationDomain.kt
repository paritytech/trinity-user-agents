package io.paritytech.polkadotapp.feature_statement_store_impl.domain.notificationAllocator

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.hexToDataByteArray
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.OperationGroupId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPolicy
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPolicyId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TxDomainId

internal const val NOTIFICATION_SLOT_DOMAIN_ID = "statement-store-notification-slot"
internal const val NOTIFICATION_SLOT_POLICY_ID = "statement-store-notification-slot"

internal val NOTIFICATION_SLOT_DOMAIN = TxDomainId(NOTIFICATION_SLOT_DOMAIN_ID)

private const val NOTIFICATION_SLOT_GROUP_PREFIX = "notification-slot:"

// The ledger hands an oracle only the group id, so the claimed account is recoverable from it alone.
internal fun AccountId.notificationSlotGroup(): OperationGroupId =
    OperationGroupId(NOTIFICATION_SLOT_GROUP_PREFIX + toString())

internal fun OperationGroupId.notificationSlotTargetOrNull(): AccountId? {
    if (!value.startsWith(NOTIFICATION_SLOT_GROUP_PREFIX)) return null

    return runCatching { value.removePrefix(NOTIFICATION_SLOT_GROUP_PREFIX).hexToDataByteArray() }.getOrNull()
}

internal fun AccountId.notificationSlotPolicy(): SubmissionPolicy =
    SubmissionPolicy(SubmissionPolicyId(NOTIFICATION_SLOT_POLICY_ID), params = this)
