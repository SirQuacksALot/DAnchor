@file:OptIn(ExperimentalFoundationApi::class)

package com.danchor.app

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Cable
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.Wifi
import androidx.compose.material3.Card
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp

/** One entry in the Connections tab - either a live-discovered desktop, a
 * persisted [SavedConnection] not currently seen this session, or both
 * merged into one [ConnectionListItem]. Tap pings it; long-press forgets it
 * (only meaningful for saved entries, but harmless to offer for live-only
 * ones too since forgetting just means "don't persist this on next ping"). */
@Composable
fun ConnectionCard(
    item: ConnectionListItem,
    onClick: () -> Unit,
    onLongClick: () -> Unit,
    modifier: Modifier = Modifier,
    secureResult: SecureResult? = null,
) {
    // "Unknown" isn't just OFFLINE - a live device nobody has ever
    // connected to yet is just as unestablished. Only a connection that's
    // both currently reachable AND previously used reads at full strength.
    val isEstablished = item.status == ConnectionStatus.ONLINE && item.isKnown
    Card(
        modifier =
            modifier
                .fillMaxWidth()
                // Unknown/offline connections read as a dimmed, slightly
                // translucent card overall, not just dimmed text - a
                // stronger "this isn't an established connection" signal
                // than color alone.
                .alpha(if (isEstablished) 1f else 0.55f)
                .combinedClickable(onClick = onClick, onLongClick = onLongClick),
    ) {
        Row(
            Modifier.padding(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(
                Modifier
                    .size(40.dp)
                    .clip(CircleShape)
                    // Neutral, not brand-colored - primary is reserved for
                    // interactive elements (buttons etc.), and this icon
                    // isn't one. Info stands in as a generic "unknown device
                    // type" glyph - there's no device-type data to pick a
                    // more specific icon from yet.
                    .background(MaterialTheme.colorScheme.onSurface.copy(alpha = 0.15f)),
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    Icons.Default.Info,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.onSurface,
                )
            }
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(item.name, style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
                    ConnectionTypeIcon(Icons.Default.Cable, active = ConnectionType.CABLE in item.types)
                    Spacer(Modifier.width(4.dp))
                    ConnectionTypeIcon(Icons.Default.Wifi, active = ConnectionType.WIFI in item.types)
                    Spacer(Modifier.width(8.dp))
                    Box(
                        Modifier
                            .size(10.dp)
                            .clip(CircleShape)
                            .background(statusColor(item.status)),
                    )
                }
                Text("${item.host}:${item.port}", style = MaterialTheme.typography.bodySmall, color = mutedTextColor())
                when (val ping = item.pingState) {
                    is PingState.Idle -> {}
                    is PingState.InFlight ->
                        Text(stringResource(R.string.ping_in_progress), style = MaterialTheme.typography.bodySmall, color = mutedTextColor())
                    is PingState.Success ->
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(
                                stringResource(R.string.rtt_ms_format, ping.rttMs),
                                style = MaterialTheme.typography.bodySmall,
                                color = mutedTextColor(),
                            )
                            // Independent of the plain ping above - a
                            // desktop can answer Ping/Pong just fine while
                            // still rejecting a Noise handshake (e.g. no
                            // matching trust secret configured yet).
                            if (secureResult is SecureResult.Secured) {
                                Spacer(Modifier.width(4.dp))
                                Icon(
                                    Icons.Default.Lock,
                                    contentDescription = stringResource(R.string.secured_content_description),
                                    tint = mutedTextColor(),
                                    modifier = Modifier.size(14.dp),
                                )
                            }
                        }
                    is PingState.Failure ->
                        Text(
                            stringResource(R.string.error_format, ping.message),
                            style = MaterialTheme.typography.bodySmall,
                            color = mutedTextColor(),
                        )
                }
            }
        }
    }
}

/** Skeleton stand-in for [ConnectionCard], shown in the grid while the
 * initial search hasn't turned up anything yet (see ConnectionsScreen.kt's
 * `isLoading`) - matches its real counterpart's shape (circle avatar + two
 * text-line bars) so the grid doesn't visibly reflow once real cards land. */
@Composable
fun ConnectionCardPlaceholder(modifier: Modifier = Modifier) {
    val transition = rememberInfiniteTransition(label = "connection-card-placeholder")
    val alpha by
        transition.animateFloat(
            initialValue = 0.2f,
            targetValue = 0.45f,
            animationSpec =
                infiniteRepeatable(
                    animation = tween(durationMillis = 800, easing = LinearEasing),
                    repeatMode = RepeatMode.Reverse,
                ),
            label = "connection-card-placeholder-alpha",
        )
    val placeholderColor = MaterialTheme.colorScheme.onSurface.copy(alpha = alpha)

    Card(modifier = modifier.fillMaxWidth()) {
        Row(
            Modifier.padding(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(
                Modifier
                    .size(40.dp)
                    .clip(CircleShape)
                    .background(placeholderColor),
            )
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Box(
                    Modifier
                        .fillMaxWidth(0.6f)
                        .height(16.dp)
                        .clip(RoundedCornerShape(4.dp))
                        .background(placeholderColor),
                )
                Spacer(Modifier.height(6.dp))
                Box(
                    Modifier
                        .fillMaxWidth(0.35f)
                        .height(12.dp)
                        .clip(RoundedCornerShape(4.dp))
                        .background(placeholderColor),
                )
            }
        }
    }
}

@Composable
private fun ConnectionTypeIcon(
    icon: ImageVector,
    active: Boolean,
) {
    Icon(
        icon,
        contentDescription = null,
        tint = if (active) MaterialTheme.colorScheme.onSurface else mutedTextColor(),
        modifier = Modifier.size(18.dp),
    )
}

private fun statusColor(status: ConnectionStatus): Color =
    when (status) {
        ConnectionStatus.ONLINE -> Color(0xFF4CAF50)
        ConnectionStatus.OFFLINE -> Color(0xFF757575)
    }
