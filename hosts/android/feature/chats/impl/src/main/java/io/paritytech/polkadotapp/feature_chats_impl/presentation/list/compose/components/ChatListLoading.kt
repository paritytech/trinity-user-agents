package io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.HorizontalDivider
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.progress.Shimmer
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
internal fun ChatListLoading() {
    Column {
        repeat(6) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .height(96.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Shimmer(Modifier.size(72.dp), shape = PolkadotTheme.shapes.full)

                Column(
                    modifier = Modifier
                        .padding(start = PolkadotTheme.spacings.mediumIncreased)
                        .fillMaxSize(),
                    verticalArrangement = Arrangement.Center
                ) {
                    Shimmer(
                        Modifier
                            .padding(top = 19.dp)
                            .width(164.dp)
                            .height(20.dp)
                    )

                    Shimmer(
                        Modifier
                            .padding(top = 6.dp, bottom = 19.dp)
                            .width(204.dp)
                            .height(32.dp)
                    )

                    HorizontalDivider(
                        color = Color(0x1FFFFFFF)
                    )
                }
            }
        }
    }
}
