package com.danchor.app

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch

// The pager has one more page than there are TabSwitcher labels - Settings
// is reachable by swipe or by the gear icon's direct jump. It still gets the
// same active/muted tab-color treatment as Verbindungen/Profil, just via an
// icon instead of a text label.
private const val SETTINGS_PAGE = 2

// IconButton reserves a 48dp touch target around its 24dp icon (12dp on each
// side) - subtracting that from the header's trailing margin keeps the icon
// GLYPH itself flush with the same edge the FAB/cards below align to,
// instead of the icon sitting 12dp further in than everything else.
private val ICON_BUTTON_INSET = 12.dp

@Composable
fun DAnchorApp(
    onStartDiscovery: ((DiscoveredDesktop) -> Unit, (String) -> Unit) -> Unit,
    onStopDiscovery: () -> Unit,
    onScanFallback: suspend () -> List<DiscoveredDesktop>,
    usbCableConnected: Boolean,
    onCheckUsbTethering: () -> Boolean,
    onCheckWifiConnected: () -> Boolean,
    onOpenTetherSettings: () -> Unit,
    themeMode: ThemeMode,
    onThemeModeChange: (ThemeMode) -> Unit,
    autoReloadEnabled: Boolean,
    onAutoReloadEnabledChange: (Boolean) -> Unit,
    usbHintHidden: Boolean,
    onUsbHintHiddenChange: (Boolean) -> Unit,
    visibleForAll: Boolean,
    onVisibleForAllChange: (Boolean) -> Unit,
    savedConnections: List<SavedConnection>,
    onConnectionPinged: (SavedConnection) -> Unit,
    onForgetConnection: (String) -> Unit,
    deviceProfileName: String,
    onDeviceProfileNameChange: (String) -> Unit,
    deviceId: String,
    pairingSecret: String,
    onRegenerateSecret: () -> Unit,
    onAdoptSecret: (String) -> Unit,
) {
    val pagerState = rememberPagerState(pageCount = { 3 })
    val scope = rememberCoroutineScope()

    fun goToPage(page: Int) {
        scope.launch { pagerState.animateScrollToPage(page) }
    }

    // Surface (not a plain Column + Modifier.background) because it also
    // establishes LocalContentColor for everything beneath it - without it,
    // every default-colored Text() in the pages below rendered invisibly
    // (same color as the background) once their own per-page Scaffold (which
    // used to provide that ambient color) was removed in favor of this
    // shared header. A plain Row replaces M3's TopAppBar here since TopAppBar
    // bakes in its own title/action insets that would stack on top of
    // screenHorizontalPadding(), leaving the header out of alignment with
    // the content below it. This header is owned by DAnchorApp, not by the
    // individual pages, so it sits above the HorizontalPager and never
    // slides with page content; only the tab labels'/gear icon's color react
    // to pagerState.currentPage.
    Surface(
        modifier = Modifier.fillMaxSize(),
        color = MaterialTheme.colorScheme.background,
    ) {
        Column(Modifier.fillMaxSize().statusBarsPadding()) {
            Row(
                modifier =
                    Modifier
                        .fillMaxWidth()
                        .padding(
                            start = screenHorizontalPadding(),
                            end = screenHorizontalPadding() - ICON_BUTTON_INSET,
                            top = 16.dp,
                            bottom = 16.dp,
                        ),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                TabSwitcher(
                    activeTab = Tab.entries.getOrNull(pagerState.currentPage),
                    onTabChange = { goToPage(it.ordinal) },
                    modifier = Modifier.weight(1f),
                )
                IconButton(onClick = { goToPage(SETTINGS_PAGE) }) {
                    Icon(
                        Icons.Default.Settings,
                        contentDescription = stringResource(R.string.settings_content_description),
                        tint =
                            if (pagerState.currentPage == SETTINGS_PAGE) {
                                MaterialTheme.colorScheme.onSurface
                            } else {
                                mutedTextColor()
                            },
                    )
                }
            }

            HorizontalPager(
                state = pagerState,
                modifier = Modifier.fillMaxSize().weight(1f),
            ) { page ->
                when (page) {
                    Tab.Connections.ordinal ->
                        ConnectionsScreen(
                            onStartDiscovery = onStartDiscovery,
                            onStopDiscovery = onStopDiscovery,
                            onScanFallback = onScanFallback,
                            usbCableConnected = usbCableConnected,
                            onCheckUsbTethering = onCheckUsbTethering,
                            onCheckWifiConnected = onCheckWifiConnected,
                            onOpenTetherSettings = onOpenTetherSettings,
                            autoReloadEnabled = autoReloadEnabled,
                            usbHintHidden = usbHintHidden,
                            onHideUsbHintPermanently = { onUsbHintHiddenChange(true) },
                            savedConnections = savedConnections,
                            onConnectionPinged = onConnectionPinged,
                            onForgetConnection = onForgetConnection,
                            pairingSecret = pairingSecret,
                        )
                    Tab.Profile.ordinal ->
                        ProfileScreen(
                            deviceProfileName = deviceProfileName,
                            onDeviceProfileNameChange = onDeviceProfileNameChange,
                            deviceId = deviceId,
                            pairingSecret = pairingSecret,
                            onRegenerateSecret = onRegenerateSecret,
                            onAdoptSecret = onAdoptSecret,
                        )
                    else ->
                        SettingsScreen(
                            themeMode = themeMode,
                            onThemeModeChange = onThemeModeChange,
                            autoReloadEnabled = autoReloadEnabled,
                            onAutoReloadEnabledChange = onAutoReloadEnabledChange,
                            usbHintHidden = usbHintHidden,
                            onUsbHintHiddenChange = onUsbHintHiddenChange,
                            visibleForAll = visibleForAll,
                            onVisibleForAllChange = onVisibleForAllChange,
                        )
                }
            }
        }
    }
}
