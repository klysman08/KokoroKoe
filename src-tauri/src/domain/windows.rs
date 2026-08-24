use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

/// The detached windows KokoroKoe owns.
///
/// This is a closed set, not a label. The frontend names a window by choosing
/// one of these variants, so it can still never supply an arbitrary webview
/// label, URL, or size: an unknown value fails to deserialize before any
/// command body runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum DetachedWindow {
    Transcript,
    Insights,
}

impl DetachedWindow {
    pub(crate) const ALL: [Self; 2] = [Self::Transcript, Self::Insights];

    /// The webview label. It is also the key the persisted window state and the
    /// per-window capability files use, so the three can never drift apart.
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Transcript => "transcript",
            Self::Insights => "insights",
        }
    }

    pub(crate) const fn index(self) -> usize {
        match self {
            Self::Transcript => 0,
            Self::Insights => 1,
        }
    }

    /// Distinct per window, because one combination cannot show and hide two
    /// windows and the second registration of a shared binding always fails.
    pub(crate) const fn default_shortcut_binding(self) -> &'static str {
        match self {
            Self::Transcript => "Ctrl+Shift+T",
            Self::Insights => "Ctrl+Shift+I",
        }
    }
}

/// A window-scoped value as it crosses to the frontend.
///
/// Serialize-only, and flattened, so the frontend sees one object carrying the
/// window alongside the value's own fields. The window is added here rather
/// than inside the appearance and shortcut structs because those are also the
/// persisted shapes: folding a window field into them would rewrite every
/// stored row for no gain.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DetachedWindowView<T> {
    pub(crate) window: DetachedWindow,
    #[serde(flatten)]
    pub(crate) value: T,
}

impl<T> DetachedWindowView<T> {
    pub(crate) const fn new(window: DetachedWindow, value: T) -> Self {
        Self { window, value }
    }
}

/// The transcript window never becomes fully invisible: the Manifest requires
/// opacity to dim the background without destroying readability, and a window
/// the user cannot see is a window they cannot recover.
pub(crate) const MINIMUM_BACKGROUND_OPACITY: f64 = 0.30;
pub(crate) const MAXIMUM_BACKGROUND_OPACITY: f64 = 1.00;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DetachedWindowAppearance {
    pub(crate) schema_version: u8,
    /// Applied to the window background only. Text is always rendered fully
    /// opaque on top of it.
    pub(crate) background_opacity: f64,
    pub(crate) always_on_top: bool,
    pub(crate) compact: bool,
}

impl Default for DetachedWindowAppearance {
    fn default() -> Self {
        Self {
            schema_version: 1,
            background_opacity: MAXIMUM_BACKGROUND_OPACITY,
            always_on_top: false,
            compact: false,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawDetachedWindowAppearance {
    schema_version: u8,
    background_opacity: f64,
    always_on_top: bool,
    compact: bool,
}

impl<'de> Deserialize<'de> for DetachedWindowAppearance {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawDetachedWindowAppearance::deserialize(deserializer)?;
        let appearance = Self {
            schema_version: raw.schema_version,
            background_opacity: raw.background_opacity,
            always_on_top: raw.always_on_top,
            compact: raw.compact,
        };
        appearance.validate().map_err(D::Error::custom)?;
        Ok(appearance)
    }
}

impl DetachedWindowAppearance {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1 || !valid_opacity(self.background_opacity) {
            return Err("window_appearance_invalid");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SetDetachedWindowAppearanceRequest {
    pub(crate) window: DetachedWindow,
    pub(crate) background_opacity: f64,
    pub(crate) always_on_top: bool,
    pub(crate) compact: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawSetDetachedWindowAppearanceRequest {
    window: DetachedWindow,
    background_opacity: f64,
    always_on_top: bool,
    compact: bool,
}

impl<'de> Deserialize<'de> for SetDetachedWindowAppearanceRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawSetDetachedWindowAppearanceRequest::deserialize(deserializer)?;
        let request = Self {
            window: raw.window,
            background_opacity: raw.background_opacity,
            always_on_top: raw.always_on_top,
            compact: raw.compact,
        };
        request.validate().map_err(D::Error::custom)?;
        Ok(request)
    }
}

impl SetDetachedWindowAppearanceRequest {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if !valid_opacity(self.background_opacity) {
            return Err("window_appearance_invalid");
        }
        Ok(())
    }

    /// Quantizes to whole percentage points so stored and emitted values are
    /// canonical regardless of slider precision.
    pub(crate) fn into_appearance(self) -> DetachedWindowAppearance {
        DetachedWindowAppearance {
            schema_version: 1,
            background_opacity: (self.background_opacity * 100.0).round() / 100.0,
            always_on_top: self.always_on_top,
            compact: self.compact,
        }
    }
}

/// Physical outer position and inner size of a window.
///
/// Physical pixels are stored deliberately: monitor bounds are reported in the
/// same units, so restoring never mixes logical and physical coordinates on a
/// scaled display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DetachedWindowGeometry {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawDetachedWindowGeometry {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl<'de> Deserialize<'de> for DetachedWindowGeometry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawDetachedWindowGeometry::deserialize(deserializer)?;
        let geometry = Self {
            x: raw.x,
            y: raw.y,
            width: raw.width,
            height: raw.height,
        };
        geometry.validate().map_err(D::Error::custom)?;
        Ok(geometry)
    }
}

impl DetachedWindowGeometry {
    /// Guards against absurd stored values. Whether the rectangle is actually
    /// reachable on the current displays is a separate check, because monitors
    /// change between runs.
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if !(MINIMUM_WINDOW_EXTENT..=MAXIMUM_WINDOW_EXTENT).contains(&self.width)
            || !(MINIMUM_WINDOW_EXTENT..=MAXIMUM_WINDOW_EXTENT).contains(&self.height)
            || !(-MAXIMUM_WINDOW_ORIGIN..=MAXIMUM_WINDOW_ORIGIN).contains(&self.x)
            || !(-MAXIMUM_WINDOW_ORIGIN..=MAXIMUM_WINDOW_ORIGIN).contains(&self.y)
        {
            return Err("window_geometry_invalid");
        }
        Ok(())
    }

    /// Reports whether enough of the window overlaps the given monitor for the
    /// user to grab it. A window restored onto a monitor that is no longer
    /// attached would otherwise be invisible and unrecoverable.
    pub(crate) fn is_reachable_on(
        &self,
        monitor_x: i32,
        monitor_y: i32,
        monitor_width: u32,
        monitor_height: u32,
    ) -> bool {
        let overlap = |start: i32, extent: u32, other_start: i32, other_extent: u32| {
            let end = start.saturating_add(extent as i32);
            let other_end = other_start.saturating_add(other_extent as i32);
            end.min(other_end).saturating_sub(start.max(other_start))
        };
        overlap(self.x, self.width, monitor_x, monitor_width) >= MINIMUM_VISIBLE_WIDTH
            && overlap(self.y, self.height, monitor_y, monitor_height) >= MINIMUM_VISIBLE_HEIGHT
    }
}

const MINIMUM_WINDOW_EXTENT: u32 = 200;
const MAXIMUM_WINDOW_EXTENT: u32 = 20_000;
const MAXIMUM_WINDOW_ORIGIN: i32 = 60_000;
const MINIMUM_VISIBLE_WIDTH: i32 = 120;
const MINIMUM_VISIBLE_HEIGHT: i32 = 60;

/// Whether the transcript window currently passes mouse input through.
///
/// This is deliberately **not** part of the persisted window state. A
/// click-through window cannot be clicked, dragged, or closed, so a stored
/// value that survived a restart or a crash would be a trap; interactivity is
/// always restored when the application starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DetachedWindowInteraction {
    pub(crate) schema_version: u8,
    pub(crate) click_through: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawDetachedWindowInteraction {
    schema_version: u8,
    click_through: bool,
}

impl<'de> Deserialize<'de> for DetachedWindowInteraction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawDetachedWindowInteraction::deserialize(deserializer)?;
        let interaction = Self {
            schema_version: raw.schema_version,
            click_through: raw.click_through,
        };
        interaction.validate().map_err(D::Error::custom)?;
        Ok(interaction)
    }
}

impl DetachedWindowInteraction {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1 {
            return Err("window_interaction_invalid");
        }
        Ok(())
    }
}

impl Default for DetachedWindowInteraction {
    fn default() -> Self {
        Self {
            schema_version: 1,
            click_through: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SetDetachedWindowInteractionRequest {
    pub(crate) window: DetachedWindow,
    pub(crate) click_through: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawSetDetachedWindowInteractionRequest {
    window: DetachedWindow,
    click_through: bool,
}

impl<'de> Deserialize<'de> for SetDetachedWindowInteractionRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawSetDetachedWindowInteractionRequest::deserialize(deserializer)?;
        Ok(Self {
            window: raw.window,
            click_through: raw.click_through,
        })
    }
}

/// A configurable system-wide show/hide binding for the transcript window.
///
/// The binding is stored canonically so the same combination always compares
/// and displays identically regardless of how the user typed it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DetachedWindowShortcut {
    pub(crate) schema_version: u8,
    pub(crate) binding: String,
    pub(crate) enabled: bool,
}

impl Default for DetachedWindowShortcut {
    fn default() -> Self {
        Self {
            schema_version: 1,
            binding: DEFAULT_SHORTCUT_BINDING.to_owned(),
            enabled: true,
        }
    }
}

pub(crate) const DEFAULT_SHORTCUT_BINDING: &str = "Ctrl+Shift+T";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawDetachedWindowShortcut {
    schema_version: u8,
    binding: String,
    enabled: bool,
}

impl<'de> Deserialize<'de> for DetachedWindowShortcut {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawDetachedWindowShortcut::deserialize(deserializer)?;
        let shortcut = Self {
            schema_version: raw.schema_version,
            binding: raw.binding,
            enabled: raw.enabled,
        };
        shortcut.validate().map_err(D::Error::custom)?;
        Ok(shortcut)
    }
}

impl DetachedWindowShortcut {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1 {
            return Err("window_shortcut_invalid");
        }
        let parsed = ParsedShortcut::parse(&self.binding)?;
        if parsed.canonical() != self.binding {
            return Err("window_shortcut_invalid");
        }
        Ok(())
    }

    pub(crate) fn parsed(&self) -> Result<ParsedShortcut, &'static str> {
        ParsedShortcut::parse(&self.binding)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SetDetachedWindowShortcutRequest {
    pub(crate) window: DetachedWindow,
    pub(crate) binding: String,
    pub(crate) enabled: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawSetDetachedWindowShortcutRequest {
    window: DetachedWindow,
    binding: String,
    enabled: bool,
}

impl<'de> Deserialize<'de> for SetDetachedWindowShortcutRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawSetDetachedWindowShortcutRequest::deserialize(deserializer)?;
        let request = Self {
            window: raw.window,
            binding: raw.binding,
            enabled: raw.enabled,
        };
        request.validate().map_err(D::Error::custom)?;
        Ok(request)
    }
}

impl SetDetachedWindowShortcutRequest {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        ParsedShortcut::parse(&self.binding).map(|_| ())
    }

    /// Accepts any spelling the user typed and stores the canonical form.
    pub(crate) fn into_shortcut(self) -> Result<DetachedWindowShortcut, &'static str> {
        let parsed = ParsedShortcut::parse(&self.binding)?;
        Ok(DetachedWindowShortcut {
            schema_version: 1,
            binding: parsed.canonical(),
            enabled: self.enabled,
        })
    }
}

/// What the user needs to know about a binding: what it is, whether it is meant
/// to be active, and whether the system actually accepted it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DetachedWindowShortcutStatus {
    pub(crate) schema_version: u8,
    pub(crate) binding: String,
    pub(crate) enabled: bool,
    /// False when another application already owns the combination.
    pub(crate) registered: bool,
}

/// A validated binding decomposed into the values the platform needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ParsedShortcut {
    pub(crate) control: bool,
    pub(crate) alt: bool,
    pub(crate) shift: bool,
    pub(crate) meta: bool,
    pub(crate) virtual_key: u16,
    key: KeyName,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyName {
    Character(char),
    Function(u8),
}

impl ParsedShortcut {
    /// Parses a binding, rejecting anything that would hijack ordinary typing.
    ///
    /// A system-wide hotkey with no `Ctrl`, `Alt`, or `Win` would capture a
    /// plain keystroke from every application on the machine, so at least one
    /// of those is required.
    pub(crate) fn parse(binding: &str) -> Result<Self, &'static str> {
        if binding.is_empty() || binding.len() > 64 {
            return Err("window_shortcut_invalid");
        }
        let mut control = false;
        let mut alt = false;
        let mut shift = false;
        let mut meta = false;
        let mut key: Option<KeyName> = None;
        for part in binding.split('+') {
            let token = part.trim();
            if token.is_empty() {
                return Err("window_shortcut_invalid");
            }
            let lowered = token.to_ascii_lowercase();
            let duplicate = match lowered.as_str() {
                "ctrl" | "control" => std::mem::replace(&mut control, true),
                "alt" => std::mem::replace(&mut alt, true),
                "shift" => std::mem::replace(&mut shift, true),
                "win" | "super" | "meta" => std::mem::replace(&mut meta, true),
                _ => {
                    if key.is_some() {
                        return Err("window_shortcut_invalid");
                    }
                    key = Some(parse_key(&lowered)?);
                    false
                }
            };
            if duplicate {
                return Err("window_shortcut_invalid");
            }
        }
        let key = key.ok_or("window_shortcut_invalid")?;
        if !(control || alt || meta) {
            return Err("window_shortcut_requires_modifier");
        }
        Ok(Self {
            control,
            alt,
            shift,
            meta,
            virtual_key: virtual_key(key),
            key,
        })
    }

    pub(crate) fn canonical(&self) -> String {
        let mut parts = Vec::new();
        if self.control {
            parts.push("Ctrl".to_owned());
        }
        if self.alt {
            parts.push("Alt".to_owned());
        }
        if self.shift {
            parts.push("Shift".to_owned());
        }
        if self.meta {
            parts.push("Win".to_owned());
        }
        parts.push(match self.key {
            KeyName::Character(value) => value.to_ascii_uppercase().to_string(),
            KeyName::Function(number) => format!("F{number}"),
        });
        parts.join("+")
    }
}

fn parse_key(lowered: &str) -> Result<KeyName, &'static str> {
    if let Some(number) = lowered.strip_prefix('f')
        && lowered.len() > 1
        && number.bytes().all(|byte| byte.is_ascii_digit())
    {
        let number: u8 = number.parse().map_err(|_| "window_shortcut_invalid")?;
        if (1..=24).contains(&number) {
            return Ok(KeyName::Function(number));
        }
        return Err("window_shortcut_invalid");
    }
    let mut characters = lowered.chars();
    match (characters.next(), characters.next()) {
        (Some(value), None) if value.is_ascii_alphanumeric() => Ok(KeyName::Character(value)),
        _ => Err("window_shortcut_invalid"),
    }
}

fn virtual_key(key: KeyName) -> u16 {
    match key {
        // Letter and digit virtual-key codes equal their uppercase ASCII value.
        KeyName::Character(value) => value.to_ascii_uppercase() as u16,
        // VK_F1 is 0x70 and the function keys are contiguous from there.
        KeyName::Function(number) => 0x6F + u16::from(number),
    }
}

/// Everything remembered about the transcript window between runs.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DetachedWindowState {
    pub(crate) schema_version: u8,
    pub(crate) appearance: DetachedWindowAppearance,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) geometry: Option<DetachedWindowGeometry>,
    pub(crate) shortcut: DetachedWindowShortcut,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawDetachedWindowState {
    schema_version: u8,
    appearance: DetachedWindowAppearance,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    geometry: Option<DetachedWindowGeometry>,
    #[serde(default)]
    shortcut: DetachedWindowShortcut,
}

impl<'de> Deserialize<'de> for DetachedWindowState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawDetachedWindowState::deserialize(deserializer)?;
        let state = Self {
            schema_version: raw.schema_version,
            appearance: raw.appearance,
            geometry: raw.geometry,
            shortcut: raw.shortcut,
        };
        state.validate().map_err(D::Error::custom)?;
        Ok(state)
    }
}

impl DetachedWindowState {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1 {
            return Err("window_state_invalid");
        }
        self.appearance.validate()?;
        if let Some(geometry) = self.geometry {
            geometry.validate()?;
        }
        self.shortcut.validate()?;
        Ok(())
    }
}

impl DetachedWindowState {
    /// The starting state for a window that has nothing stored yet.
    ///
    /// Each window gets its own default binding: two windows sharing one
    /// combination would mean the second registration is always refused, and
    /// the user would see a taken-shortcut warning they did not cause.
    pub(crate) fn default_for(window: DetachedWindow) -> Self {
        Self {
            schema_version: 1,
            appearance: DetachedWindowAppearance::default(),
            geometry: None,
            shortcut: DetachedWindowShortcut {
                schema_version: 1,
                binding: window.default_shortcut_binding().to_owned(),
                enabled: true,
            },
        }
    }
}

fn deserialize_optional_non_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

fn valid_opacity(value: f64) -> bool {
    value.is_finite() && (MINIMUM_BACKGROUND_OPACITY..=MAXIMUM_BACKGROUND_OPACITY).contains(&value)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_default_appearance_is_fully_opaque_and_unpinned() {
        let appearance = DetachedWindowAppearance::default();

        appearance.validate().unwrap();
        assert_eq!(appearance.background_opacity, MAXIMUM_BACKGROUND_OPACITY);
        assert!(!appearance.always_on_top);
        assert!(!appearance.compact);
    }

    #[test]
    fn opacity_outside_the_readable_range_is_rejected() {
        for opacity in [0.0, 0.29, 1.01, f64::NAN, f64::INFINITY] {
            let request = json!({
                "backgroundOpacity": opacity,
                "alwaysOnTop": false,
                "compact": false
            });
            assert!(
                serde_json::from_value::<SetDetachedWindowAppearanceRequest>(request).is_err(),
                "opacity {opacity} must be rejected"
            );
        }
    }

    #[test]
    fn accepted_requests_quantize_to_whole_percentage_points() {
        let request: SetDetachedWindowAppearanceRequest = serde_json::from_value(json!({
            "window": "transcript",
            "backgroundOpacity": 0.6789,
            "alwaysOnTop": true,
            "compact": true
        }))
        .unwrap();

        let appearance = request.into_appearance();

        assert_eq!(appearance.background_opacity, 0.68);
        assert!(appearance.always_on_top);
        assert!(appearance.compact);
        appearance.validate().unwrap();
    }

    #[test]
    fn absurd_geometry_is_rejected_but_plausible_geometry_survives() {
        let valid: DetachedWindowGeometry =
            serde_json::from_value(json!({"x": -1_920, "y": 40, "width": 520, "height": 720}))
                .unwrap();
        valid.validate().unwrap();

        for geometry in [
            json!({"x": 0, "y": 0, "width": 10, "height": 720}),
            json!({"x": 0, "y": 0, "width": 520, "height": 99_999}),
            json!({"x": 999_999, "y": 0, "width": 520, "height": 720}),
        ] {
            assert!(serde_json::from_value::<DetachedWindowGeometry>(geometry).is_err());
        }
    }

    #[test]
    fn a_window_is_reachable_only_when_enough_of_it_overlaps_a_monitor() {
        let geometry = DetachedWindowGeometry {
            x: 100,
            y: 100,
            width: 520,
            height: 720,
        };

        assert!(geometry.is_reachable_on(0, 0, 1_920, 1_080));
        // Fully off to the right of the only remaining monitor.
        assert!(!geometry.is_reachable_on(2_000, 0, 1_920, 1_080));
        // A monitor that was unplugged leaves the window on a negative origin.
        let detached = DetachedWindowGeometry {
            x: -1_900,
            y: 100,
            width: 520,
            height: 720,
        };
        assert!(!detached.is_reachable_on(0, 0, 1_920, 1_080));
        assert!(detached.is_reachable_on(-1_920, 0, 1_920, 1_080));
        // Only a sliver visible is treated as unreachable.
        let sliver = DetachedWindowGeometry {
            x: 1_900,
            y: 100,
            width: 520,
            height: 720,
        };
        assert!(!sliver.is_reachable_on(0, 0, 1_920, 1_080));
    }

    #[test]
    fn persisted_state_round_trips_and_rejects_invalid_members() {
        let state: DetachedWindowState = serde_json::from_value(json!({
            "schemaVersion": 1,
            "appearance": {
                "schemaVersion": 1,
                "backgroundOpacity": 0.7,
                "alwaysOnTop": true,
                "compact": false
            },
            "geometry": {"x": 10, "y": 20, "width": 520, "height": 720}
        }))
        .unwrap();
        state.validate().unwrap();
        assert_eq!(state.appearance.background_opacity, 0.7);

        let restored: DetachedWindowState =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        assert_eq!(restored, state);

        let without_geometry: DetachedWindowState = serde_json::from_value(json!({
            "schemaVersion": 1,
            "appearance": {
                "schemaVersion": 1,
                "backgroundOpacity": 1.0,
                "alwaysOnTop": false,
                "compact": false
            }
        }))
        .unwrap();
        assert!(without_geometry.geometry.is_none());

        let unreadable = json!({
            "schemaVersion": 1,
            "appearance": {
                "schemaVersion": 1,
                "backgroundOpacity": 0.0,
                "alwaysOnTop": false,
                "compact": false
            }
        });
        assert!(serde_json::from_value::<DetachedWindowState>(unreadable).is_err());
    }

    #[test]
    fn a_binding_without_ctrl_alt_or_win_is_refused() {
        // A system-wide hotkey on a bare or shift-only key would capture that
        // keystroke from every application on the machine.
        for binding in ["T", "Shift+T", "F5", "Shift+F5"] {
            assert_eq!(
                ParsedShortcut::parse(binding).unwrap_err(),
                "window_shortcut_requires_modifier",
                "{binding} must be refused"
            );
        }
        assert!(ParsedShortcut::parse("Ctrl+Shift+T").is_ok());
        assert!(ParsedShortcut::parse("Alt+F5").is_ok());
        assert!(ParsedShortcut::parse("Win+K").is_ok());
    }

    #[test]
    fn malformed_bindings_are_refused() {
        for binding in [
            "",
            "Ctrl+",
            "+T",
            "Ctrl++T",
            "Ctrl+Ctrl+T",
            "Ctrl+T+K",
            "Ctrl+F0",
            "Ctrl+F25",
            "Ctrl+Tab",
            "Ctrl+é",
        ] {
            assert!(
                ParsedShortcut::parse(binding).is_err(),
                "{binding} must be refused"
            );
        }
    }

    #[test]
    fn bindings_canonicalize_regardless_of_spelling() {
        for (typed, canonical) in [
            ("ctrl+shift+t", "Ctrl+Shift+T"),
            ("SHIFT + CONTROL + t", "Ctrl+Shift+T"),
            ("alt+f9", "Alt+F9"),
            ("super+k", "Win+K"),
            ("control+alt+shift+win+9", "Ctrl+Alt+Shift+Win+9"),
        ] {
            assert_eq!(ParsedShortcut::parse(typed).unwrap().canonical(), canonical);
        }
    }

    #[test]
    fn virtual_keys_match_the_platform_numbering() {
        assert_eq!(ParsedShortcut::parse("Ctrl+A").unwrap().virtual_key, 0x41);
        assert_eq!(ParsedShortcut::parse("Ctrl+0").unwrap().virtual_key, 0x30);
        assert_eq!(ParsedShortcut::parse("Ctrl+F1").unwrap().virtual_key, 0x70);
        assert_eq!(ParsedShortcut::parse("Ctrl+F24").unwrap().virtual_key, 0x87);
    }

    #[test]
    fn a_shortcut_request_stores_the_canonical_binding() {
        let request: SetDetachedWindowShortcutRequest = serde_json::from_value(json!({
            "window": "insights",
            "binding": "shift+ctrl+t",
            "enabled": true
        }))
        .unwrap();

        let shortcut = request.into_shortcut().unwrap();

        assert_eq!(shortcut.binding, "Ctrl+Shift+T");
        assert!(shortcut.enabled);
        shortcut.validate().unwrap();
    }

    #[test]
    fn a_stored_shortcut_must_already_be_canonical() {
        let non_canonical = json!({
            "schemaVersion": 1,
            "binding": "shift+ctrl+t",
            "enabled": true
        });
        assert!(serde_json::from_value::<DetachedWindowShortcut>(non_canonical).is_err());

        let modifierless = json!({
            "schemaVersion": 1,
            "binding": "T",
            "enabled": true
        });
        assert!(serde_json::from_value::<DetachedWindowShortcut>(modifierless).is_err());
    }

    #[test]
    fn state_stored_before_shortcuts_existed_still_reads() {
        let legacy: DetachedWindowState = serde_json::from_value(json!({
            "schemaVersion": 1,
            "appearance": {
                "schemaVersion": 1,
                "backgroundOpacity": 0.8,
                "alwaysOnTop": false,
                "compact": false
            }
        }))
        .unwrap();

        assert_eq!(legacy.shortcut.binding, DEFAULT_SHORTCUT_BINDING);
        assert!(legacy.shortcut.enabled);
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let request = json!({
            "backgroundOpacity": 0.8,
            "alwaysOnTop": false,
            "compact": false,
            "clickThrough": true
        });
        assert!(serde_json::from_value::<SetDetachedWindowAppearanceRequest>(request).is_err());
    }
}
