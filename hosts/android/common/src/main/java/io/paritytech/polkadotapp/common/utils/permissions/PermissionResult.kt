package io.paritytech.polkadotapp.common.utils.permissions

enum class PermissionResult {
    GRANTED, DENIED, DENIED_FOREVER
}

fun PermissionResult.isGranted() = this == PermissionResult.GRANTED

fun List<PermissionResult>.flattenToMostRelevant(): PermissionResult {
    val isAnyForeverDenied = any { it == PermissionResult.DENIED_FOREVER }
    if (isAnyForeverDenied) return PermissionResult.DENIED_FOREVER

    return firstOrNull { it != PermissionResult.GRANTED } ?: PermissionResult.GRANTED
}
