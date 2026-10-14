package io.paritytech.polkadotapp.tools_authentication_api.domain

import io.paritytech.polkadotapp.common.domain.errors.UserCancellation

class AuthenticationCancelledException(message: String = "Authentication has been cancelled") : Exception(message), UserCancellation
