@file:OptIn(ExperimentalMaterial3Api::class)

package com.danchor.app

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Usb
import androidx.compose.material.icons.filled.UsbOff
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

// How long to give mDNS before falling back to a unicast subnet scan.
private const val MDNS_GRACE_PERIOD_MS = 4000L
private const val AUTO_RELOAD_INTERVAL_MS = 10000L

// The grid always shows at least this many cells while a scan is running -
// known/saved connections keep their own real cards (never replaced), and
// any remaining slots up to this count are filled with loading placeholders
// instead of a couple of stray shimmer cards floating in otherwise-empty
// space.
private const val TARGET_SLOT_COUNT = 6

@Composable
fun ConnectionsScreen(
    onStartDiscovery: ((DiscoveredDesktop) -> Unit, (String) -> Unit) -> Unit,
    onStopDiscovery: () -> Unit,
    onScanFallback: suspend () -> List<DiscoveredDesktop>,
    usbCableConnected: Boolean,
    onCheckUsbTethering: () -> Boolean,
    onCheckWifiConnected: () -> Boolean,
    onOpenTetherSettings: () -> Unit,
    autoReloadEnabled: Boolean,
    usbHintHidden: Boolean,
    onHideUsbHintPermanently: () -> Unit,
    savedConnections: List<SavedConnection>,
    onConnectionPinged: (SavedConnection) -> Unit,
    onForgetConnection: (String) -> Unit,
    pairingSecret: String,
) {
    val context = LocalContext.current
    val searchingText = stringResource(R.string.status_searching_mdns)
    val noDesktopsFoundText = stringResource(R.string.status_no_desktops_found)
    val desktopsFoundText = stringResource(R.string.status_desktops_found)

    val devices = remember { mutableStateMapOf<String, DiscoveredDesktop>() }
    val pingStates = remember { mutableStateMapOf<String, PingState>() }
    // Kept separate from pingStates - a plain ping succeeding (device found,
    // name/RTT known) and a secure session succeeding (desktop holds a
    // matching trust secret) are independent outcomes, see SecureResult's
    // doc comment.
    val secureStates = remember { mutableStateMapOf<String, SecureResult>() }
    var statusText by remember { mutableStateOf(searchingText) }
    // Starts true - a search (the initial mDNS grace-period wait, covered
    // below) is already under way from first composition, not just once the
    // subnet-scan fallback kicks in.
    var isScanning by remember { mutableStateOf(true) }
    var usbTetheringActive by remember { mutableStateOf(false) }
    var wifiConnected by remember { mutableStateOf(false) }
    var showManualConnectDialog by remember { mutableStateOf(false) }
    var usbHintDismissedThisSession by remember { mutableStateOf(false) }
    var dontShowUsbHintAgain by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()

    fun handleDeviceFound(device: DiscoveredDesktop) {
        devices[device.name] = device
        statusText = desktopsFoundText
    }

    fun handleDeviceLost(name: String) {
        devices.remove(name)
    }

    fun reload() {
        scope.launch {
            isScanning = true
            try {
                // Restart mDNS discovery too, not just the subnet-scan
                // fallback - the passive NsdManager listener started once in
                // DisposableEffect below doesn't reliably re-probe on its
                // own (this is why only reopening the app or navigating away
                // and back, both of which tear down and recreate that
                // listener from scratch, seemed to "actually" find things).
                // Stopping and restarting it here gives a manual/auto reload
                // the same fresh-discovery power.
                onStopDiscovery()
                onStartDiscovery(::handleDeviceFound, ::handleDeviceLost)

                val found = onScanFallback()
                devices.entries.filter { it.value.source == DesktopSource.Scan }.forEach { devices.remove(it.key) }
                found.forEach { devices[it.name] = it }
                statusText = if (devices.isEmpty()) noDesktopsFoundText else desktopsFoundText
            } finally {
                isScanning = false
            }
        }
    }

    fun pingItem(item: ConnectionListItem) {
        pingStates[item.id] = PingState.InFlight
        secureStates.remove(item.id)
        scope.launch {
            val result = pingDevice(context, item.host, item.port)
            pingStates[item.id] = result
            if (result is PingState.Success) {
                // Self-heals names saved before broadcasting existed (or
                // when scan only had an IP to go on) - every successful
                // re-ping refreshes the name to whatever's currently
                // broadcast, without needing an explicit migration.
                val name = result.deviceName.ifBlank { item.name }
                onConnectionPinged(SavedConnection(item.id, name, item.host, item.port, System.currentTimeMillis()))

                // Only worth attempting once the plain ping confirms the
                // device is actually there - a secure-session failure never
                // undoes the plain ping's own success above.
                secureStates[item.id] = establishSecureSession(context, item.host, item.port, pairingSecret)
            }
        }
    }

    DisposableEffect(Unit) {
        onStartDiscovery(::handleDeviceFound, ::handleDeviceLost)
        onDispose { onStopDiscovery() }
    }

    // mDNS discovery can be blocked by WiFi routers that don't forward
    // multicast between wireless clients even though unicast works fine -
    // fall back to unicast-probing the local subnet if it hasn't found
    // anything after a grace period.
    LaunchedEffect(Unit) {
        delay(MDNS_GRACE_PERIOD_MS)
        if (devices.isEmpty()) {
            reload()
        } else {
            // mDNS already found something within the grace period - no
            // fallback scan needed, so nothing else will clear isScanning.
            isScanning = false
        }
    }

    // Polls rather than subscribing to a ConnectivityManager.NetworkCallback
    // - see Networking.kt's findUsbNetwork() doc for why.
    LaunchedEffect(Unit) {
        while (true) {
            usbTetheringActive = onCheckUsbTethering()
            wifiConnected = onCheckWifiConnected()
            delay(2000)
        }
    }

    LaunchedEffect(autoReloadEnabled) {
        if (!autoReloadEnabled) return@LaunchedEffect
        while (true) {
            delay(AUTO_RELOAD_INTERVAL_MS)
            reload()
        }
    }

    val currentTypes =
        buildSet {
            if (usbTetheringActive) add(ConnectionType.CABLE)
            if (wifiConnected) add(ConnectionType.WIFI)
        }
    val savedIds = savedConnections.map { it.id }.toSet()
    val liveItems =
        devices.values.map { device ->
            ConnectionListItem(
                id = device.name,
                name = device.name,
                host = device.host.hostAddress ?: device.name,
                port = device.port,
                status = ConnectionStatus.ONLINE,
                types = currentTypes,
                pingState = pingStates[device.name] ?: PingState.Idle,
                isKnown = device.name in savedIds,
            )
        }
    val savedOnlyItems =
        savedConnections.filter { it.id !in devices.keys }.map { saved ->
            ConnectionListItem(
                id = saved.id,
                name = saved.name,
                host = saved.host,
                port = saved.port,
                status = ConnectionStatus.OFFLINE,
                types = emptySet(),
                pingState = pingStates[saved.id] ?: PingState.Idle,
                isKnown = true,
            )
        }
    val items = (liveItems + savedOnlyItems).sortedBy { it.status == ConnectionStatus.OFFLINE }
    // Known/saved connections always keep their own real card - placeholders
    // only ever fill the REMAINING slots up to TARGET_SLOT_COUNT, and only
    // while a scan (any reload, not just the very first one) is actually
    // running.
    val placeholderCount = if (isScanning) (TARGET_SLOT_COUNT - items.size).coerceAtLeast(0) else 0

    Scaffold(
        floatingActionButton = {
            // Scaffold's default FAB margin is a fixed 16dp, out of step with
            // screenHorizontalPadding() everything else aligns to - override
            // it explicitly so the FAB lines up with the cards/header too.
            ExtendedFloatingActionButton(
                onClick = { showManualConnectDialog = true },
                icon = { Icon(Icons.Default.Add, contentDescription = null) },
                text = { Text(stringResource(R.string.manual_connect_button)) },
                modifier = Modifier.padding(end = screenHorizontalPadding() - 16.dp, bottom = 16.dp),
            )
        },
    ) { padding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(horizontal = screenHorizontalPadding(), vertical = 16.dp),
        ) {
            if (usbCableConnected && !usbTetheringActive && !usbHintHidden && !usbHintDismissedThisSession) {
                UsbTetheringHintCard(
                    onOpenTetherSettings = onOpenTetherSettings,
                    dontShowAgainChecked = dontShowUsbHintAgain,
                    onDontShowAgainCheckedChange = { checked ->
                        dontShowUsbHintAgain = checked
                        // Commit immediately rather than waiting for the X
                        // dismiss - otherwise checking the box then leaving
                        // the card any other way (unplugging the cable,
                        // tapping "Tethering-Einstellungen öffnen") silently
                        // discards it, and the hint comes back next time.
                        if (checked) onHideUsbHintPermanently()
                    },
                    onDismiss = { usbHintDismissedThisSession = true },
                )
                Spacer(Modifier.height(8.dp))
            }

            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    if (usbTetheringActive) Icons.Default.Usb else Icons.Default.UsbOff,
                    contentDescription =
                        if (usbTetheringActive) {
                            stringResource(R.string.usb_connected_description)
                        } else {
                            stringResource(R.string.usb_not_connected_description)
                        },
                    tint = mutedTextColor(),
                    modifier = Modifier.size(18.dp),
                )
                Spacer(Modifier.width(8.dp))
                Text(statusText, color = mutedTextColor())
            }
            Spacer(Modifier.height(8.dp))
            val gridColumns = if (isTabletWidth()) 2 else 1
            PullToRefreshBox(
                isRefreshing = isScanning,
                onRefresh = { reload() },
                modifier = Modifier.weight(1f),
            ) {
                LazyVerticalGrid(
                    columns = GridCells.Fixed(gridColumns),
                    modifier = Modifier.fillMaxSize(),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    items(items, key = { it.id }) { item ->
                        ConnectionCard(
                            item = item,
                            secureResult = secureStates[item.id],
                            onClick = { pingItem(item) },
                            onLongClick = { onForgetConnection(item.id) },
                        )
                    }
                    items(placeholderCount) { ConnectionCardPlaceholder() }
                }
            }
        }

        if (showManualConnectDialog) {
            ManualConnectDialog(
                onDismiss = { showManualConnectDialog = false },
                onConnected = { device ->
                    devices[device.name] = device
                    showManualConnectDialog = false
                },
            )
        }
    }
}

@Composable
private fun UsbTetheringHintCard(
    onOpenTetherSettings: () -> Unit,
    dontShowAgainChecked: Boolean,
    onDontShowAgainCheckedChange: (Boolean) -> Unit,
    onDismiss: () -> Unit,
) {
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(12.dp)) {
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.Top,
            ) {
                Text(
                    stringResource(R.string.usb_hint_title),
                    Modifier.weight(1f),
                )
                IconButton(onClick = onDismiss, modifier = Modifier.size(24.dp)) {
                    Icon(Icons.Default.Close, contentDescription = stringResource(R.string.close_content_description))
                }
            }
            Text(stringResource(R.string.usb_hint_body), color = mutedTextColor())
            Spacer(Modifier.height(8.dp))
            Button(onClick = onOpenTetherSettings) {
                Text(stringResource(R.string.usb_hint_open_settings))
            }
            Spacer(Modifier.height(4.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Checkbox(checked = dontShowAgainChecked, onCheckedChange = onDontShowAgainCheckedChange)
                Text(stringResource(R.string.usb_hint_dont_show_again), color = mutedTextColor())
            }
        }
    }
}
