package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_statement_store_api.domain.StatementStoreMessageProver

/** Who signs a chat request statement, and the delivery state that signing it results in. */
class ChatRequestDeliverySigner(
    val prover: StatementStoreMessageProver,
    val delivery: ChatRequest.Delivery,
)
