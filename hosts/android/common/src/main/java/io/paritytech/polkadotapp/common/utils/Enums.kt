package io.paritytech.polkadotapp.common.utils

inline fun <reified E : Enum<E>> enumValueOfOrNull(raw: String): E? = runCatching { enumValueOf<E>(raw) }.getOrNull()
