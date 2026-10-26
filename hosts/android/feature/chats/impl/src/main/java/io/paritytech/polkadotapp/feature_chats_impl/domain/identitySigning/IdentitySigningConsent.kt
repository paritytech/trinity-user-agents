package io.paritytech.polkadotapp.feature_chats_impl.domain.identitySigning

import io.paritytech.polkadotapp.common.domain.model.AccountId

/** The user's answer to letting their identity account sign one chat's statements (chat-spec RFC-0005 §4). */
enum class IdentitySigningConsent {
    REQUESTED,
    GRANTED,
    DECLINED,
}

class IdentitySigningConsentRequired(contactAccountId: AccountId) :
    Throwable("No anonymous slot is free for the chat with $contactAccountId; waiting for the user to allow the identity signer")

class IdentitySigningDeclined(contactAccountId: AccountId) :
    Throwable("The user declined signing the chat with $contactAccountId by the identity account")
