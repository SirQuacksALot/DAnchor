@file:OptIn(ExperimentalTextApi::class)

package com.danchor.app

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

// DAnchor brand palette from the README/logo - same hue/saturation, two
// lightness steps. ec7979 is the primary (dark enough for white text/icons
// on top, per the badge convention); f2a2a2 is the lighter secondary, kept
// out of large text-bearing surfaces since it fails the light-text contrast
// threshold. Also covers the *Container roles (FAB, filled cards, etc. pull
// from these, not primary/secondary directly) so nothing falls back to
// Material3's default purple baseline.
private val DAnchorPrimary = Color(0xFFEC7979)
private val DAnchorSecondary = Color(0xFFF2A2A2)

// Card (and Dialog, etc.) pull their container color from the
// surfaceContainer* roles, not surface/surfaceVariant - leaving them unset
// falls back to Material3's baseline purple-gray tonal scale, which reads as
// a jarringly light gray against the pure black background. Pin the whole
// scale to a near-black gray progression instead so list items and dialogs
// stay dark but still distinguishable from the background and each other.
private val DAnchorDarkSurfaceVariant = Color(0xFF0F0F0F)
private val DAnchorSurfaceContainerLowest = Color(0xFF020202)
private val DAnchorSurfaceContainerLow = Color(0xFF080808)
private val DAnchorSurfaceContainer = Color(0xFF0A0A0A)
private val DAnchorSurfaceContainerHigh = Color(0xFF0F0F0F)
private val DAnchorSurfaceContainerHighest = Color(0xFF131313)

val DAnchorDarkColorScheme =
    darkColorScheme(
        primary = DAnchorPrimary,
        onPrimary = Color.White,
        primaryContainer = DAnchorPrimary,
        onPrimaryContainer = Color.White,
        secondary = DAnchorSecondary,
        onSecondary = Color.Black,
        secondaryContainer = DAnchorSecondary,
        onSecondaryContainer = Color.Black,
        background = Color.Black,
        onBackground = Color.White,
        surface = Color.Black,
        onSurface = Color.White,
        surfaceVariant = DAnchorDarkSurfaceVariant,
        onSurfaceVariant = Color(0xFFCCCCCC),
        surfaceContainerLowest = DAnchorSurfaceContainerLowest,
        surfaceContainerLow = DAnchorSurfaceContainerLow,
        surfaceContainer = DAnchorSurfaceContainer,
        surfaceContainerHigh = DAnchorSurfaceContainerHigh,
        surfaceContainerHighest = DAnchorSurfaceContainerHighest,
    )

val DAnchorLightColorScheme =
    lightColorScheme(
        primary = DAnchorPrimary,
        onPrimary = Color.White,
        primaryContainer = DAnchorPrimary,
        onPrimaryContainer = Color.White,
        secondary = DAnchorSecondary,
        onSecondary = Color.Black,
        secondaryContainer = DAnchorSecondary,
        onSecondaryContainer = Color.Black,
    )

/** TextField's filled style paints a container fill by default - shared by
 * every text input in the app, which are all styled as underline-only. */
@Composable
fun transparentTextFieldColors() =
    TextFieldDefaults.colors(
        focusedContainerColor = Color.Transparent,
        unfocusedContainerColor = Color.Transparent,
        disabledContainerColor = Color.Transparent,
    )

/** App-wide emphasis rule: important content is the main text color at full
 * strength, secondary/inactive content is the same color dimmed - white-vs-
 * gray in dark mode, black-vs-gray in light mode. Mirrors what TabSwitcher.kt
 * already does for the active/inactive tab label. */
@Composable
fun mutedTextColor() = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.5f)

// Standard Android tablet-width breakpoint (matches the sw600dp resource
// qualifier convention) - shared by every width-responsive layout choice
// (grid columns in ConnectionsScreen, screen margins below) so they all
// agree on what counts as "tablet width".
private const val TABLET_BREAKPOINT_DP = 600

@Composable
fun isTabletWidth(): Boolean = LocalConfiguration.current.screenWidthDp >= TABLET_BREAKPOINT_DP

/** App-wide horizontal screen margin - wider on tablet-width screens, since a
 * flat margin reads as cramped on this app's primary tablet target but would
 * look oversized on a phone. */
@Composable
fun screenHorizontalPadding(): Dp = if (isTabletWidth()) 40.dp else 24.dp

// Inter (SIL OFL 1.1, license text bundled at res/raw/inter_ofl.txt) - second
// candidate tried live for the app-wide typography pass (see .ai/tasks.toon):
// the first candidate, Space Grotesk, was rejected specifically over its
// capital D shape. Inter is a widely-used technical/UI sans (GitHub, Figma,
// etc.) with a more conventional letterform set. Ships as a single
// variable-weight TTF, so the same @font resource is referenced multiple
// times below with a different `wght` axis value each time - the standard
// Compose pattern for variable fonts.
private val InterFamily =
    FontFamily(
        Font(R.font.inter, FontWeight.Normal, variationSettings = FontVariation.Settings(FontVariation.weight(400))),
        Font(R.font.inter, FontWeight.Medium, variationSettings = FontVariation.Settings(FontVariation.weight(500))),
        Font(R.font.inter, FontWeight.SemiBold, variationSettings = FontVariation.Settings(FontVariation.weight(600))),
        Font(R.font.inter, FontWeight.Bold, variationSettings = FontVariation.Settings(FontVariation.weight(700))),
    )

private val defaultTypography = Typography()

/** Default Material3 type scale (sizes/line-heights/weights) with every slot's
 * font family swapped to [InterFamily] - the scale itself isn't being
 * redesigned here, just the typeface. */
val DAnchorTypography =
    Typography(
        displayLarge = defaultTypography.displayLarge.copy(fontFamily = InterFamily),
        displayMedium = defaultTypography.displayMedium.copy(fontFamily = InterFamily),
        displaySmall = defaultTypography.displaySmall.copy(fontFamily = InterFamily),
        headlineLarge = defaultTypography.headlineLarge.copy(fontFamily = InterFamily),
        headlineMedium = defaultTypography.headlineMedium.copy(fontFamily = InterFamily),
        headlineSmall = defaultTypography.headlineSmall.copy(fontFamily = InterFamily),
        titleLarge = defaultTypography.titleLarge.copy(fontFamily = InterFamily),
        titleMedium = defaultTypography.titleMedium.copy(fontFamily = InterFamily),
        titleSmall = defaultTypography.titleSmall.copy(fontFamily = InterFamily),
        bodyLarge = defaultTypography.bodyLarge.copy(fontFamily = InterFamily),
        bodyMedium = defaultTypography.bodyMedium.copy(fontFamily = InterFamily),
        bodySmall = defaultTypography.bodySmall.copy(fontFamily = InterFamily),
        labelLarge = defaultTypography.labelLarge.copy(fontFamily = InterFamily),
        labelMedium = defaultTypography.labelMedium.copy(fontFamily = InterFamily),
        labelSmall = defaultTypography.labelSmall.copy(fontFamily = InterFamily),
    )
