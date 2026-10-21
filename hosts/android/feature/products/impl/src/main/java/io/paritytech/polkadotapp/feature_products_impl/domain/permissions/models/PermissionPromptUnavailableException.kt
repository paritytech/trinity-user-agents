package io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models

/**
 * No screen got an answer from the user: the app is not in front, or the prompt closed before the user answered.
 * This is not a refusal. Callers must not keep it as a decision, so the next request asks again.
 */
class PermissionPromptUnavailableException(reason: String) : Exception(reason)
