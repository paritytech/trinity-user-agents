package io.paritytech.polkadotapp.feature_chats_impl.domain.compaction

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.HopTransferRetryState
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.WithRetryState

class CompactionExpansion(
    val commitId: ChatMessageId,
    val contactAccountId: AccountId?,
    val commit: ChatMessage.Content.CompactionCommit?,
    override val retryState: HopTransferRetryState
) : WithRetryState
