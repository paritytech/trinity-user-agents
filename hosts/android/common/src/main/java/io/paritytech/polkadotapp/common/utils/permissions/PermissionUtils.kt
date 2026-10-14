package io.paritytech.polkadotapp.common.utils.permissions

import android.content.Context
import android.content.pm.PackageManager
import androidx.core.content.ContextCompat
import androidx.fragment.app.Fragment

fun Fragment.hasPermission(permission: String): Boolean = requireContext().hasPermission(permission)

fun Context.hasPermission(permission: String): Boolean {
    return ContextCompat.checkSelfPermission(this, permission) == PackageManager.PERMISSION_GRANTED
}
