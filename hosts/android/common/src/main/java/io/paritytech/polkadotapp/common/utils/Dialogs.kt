package io.paritytech.polkadotapp.common.utils

import android.content.DialogInterface
import android.view.View

context(dialogInterface: DialogInterface)
fun View.setDismissingClickListener(listener: (View) -> Unit) {
    setOnClickListener {
        listener.invoke(it)

        dialogInterface.dismiss()
    }
}
