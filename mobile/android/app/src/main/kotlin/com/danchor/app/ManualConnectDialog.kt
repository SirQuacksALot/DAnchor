package com.danchor.app

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TextField
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.danchor_ffi.defaultPort
import java.net.InetAddress

@Composable
fun ManualConnectDialog(
    onDismiss: () -> Unit,
    onConnected: (DiscoveredDesktop) -> Unit,
) {
    val context = LocalContext.current
    var host by remember { mutableStateOf("") }
    var port by remember { mutableStateOf(defaultPort().toString()) }
    var state by remember { mutableStateOf<PingState>(PingState.Idle) }
    val scope = rememberCoroutineScope()

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.manual_connect_button)) },
        text = {
            Column {
                Text(
                    stringResource(R.string.manual_connect_description),
                    style = MaterialTheme.typography.bodySmall,
                    color = mutedTextColor(),
                )
                Spacer(Modifier.height(8.dp))
                TextField(
                    value = host,
                    onValueChange = { host = it },
                    label = { Text(stringResource(R.string.ip_label)) },
                    shape = RectangleShape,
                    colors = transparentTextFieldColors(),
                    modifier = Modifier.fillMaxWidth(),
                )
                Spacer(Modifier.height(8.dp))
                TextField(
                    value = port,
                    onValueChange = { port = it },
                    label = { Text(stringResource(R.string.port_label)) },
                    shape = RectangleShape,
                    colors = transparentTextFieldColors(),
                    modifier = Modifier.fillMaxWidth(),
                )
                Spacer(Modifier.height(8.dp))
                when (val s = state) {
                    is PingState.Idle -> {}
                    is PingState.InFlight -> Text(stringResource(R.string.connecting_label), color = mutedTextColor())
                    is PingState.Success ->
                        Text(stringResource(R.string.connected_rtt_format, s.rttMs), color = mutedTextColor())
                    is PingState.Failure ->
                        Text(stringResource(R.string.error_format, s.message), color = mutedTextColor())
                }
            }
        },
        confirmButton = {
            Button(onClick = {
                val parsedPort = port.toIntOrNull()
                if (host.isNotBlank() && parsedPort != null) {
                    state = PingState.InFlight
                    scope.launch {
                        val result = pingDevice(context, host, parsedPort)
                        state = result
                        if (result is PingState.Success) {
                            val address = withContext(Dispatchers.IO) { InetAddress.getByName(host) }
                            val name = result.deviceName.ifBlank { host }
                            onConnected(DiscoveredDesktop(name, address, parsedPort, DesktopSource.Manual))
                        }
                    }
                }
            }) {
                Text(stringResource(R.string.connect_button))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text(stringResource(R.string.cancel_button))
            }
        },
    )
}
