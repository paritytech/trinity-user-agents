package io.paritytech.polkadotapp.common.utils.progressStallReport

import androidx.annotation.StringRes
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.design.components.text.NovaText

/**
 * A step the user can be shown while it runs. The report draws the row around it - status glyph, indent, elapsed time
 * - and sets the text style, so an implementation only has to name itself.
 */
interface StallableRegion {
    @Composable
    fun Label()
}

class ReportAsText(@param:StringRes private val label: Int) : StallableRegion {
    @Composable
    override fun Label() {
        NovaText(text = stringResource(label))
    }
}
