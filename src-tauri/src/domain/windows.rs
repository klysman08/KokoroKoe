use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

/// The transcript window never becomes fully invisible: the Manifest requires
/// opacity to dim the background without destroying readability, and a window
/// the user cannot see is a window they cannot recover.
pub(crate) const MINIMUM_BACKGROUND_OPACITY: f64 = 0.30;
pub(crate) const MAXIMUM_BACKGROUND_OPACITY: f64 = 1.00;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TranscriptWindowAppearance {
    pub(crate) schema_version: u8,
    /// Applied to the window background only. Text is always rendered fully
    /// opaque on top of it.
    pub(crate) background_opacity: f64,
    pub(crate) always_on_top: bool,
    pub(crate) compact: bool,
}

impl Default for TranscriptWindowAppearance {
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
struct RawTranscriptWindowAppearance {
    schema_version: u8,
    background_opacity: f64,
    always_on_top: bool,
    compact: bool,
}

impl<'de> Deserialize<'de> for TranscriptWindowAppearance {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawTranscriptWindowAppearance::deserialize(deserializer)?;
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

impl TranscriptWindowAppearance {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1 || !valid_opacity(self.background_opacity) {
            return Err("window_appearance_invalid");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SetTranscriptWindowAppearanceRequest {
    pub(crate) background_opacity: f64,
    pub(crate) always_on_top: bool,
    pub(crate) compact: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawSetTranscriptWindowAppearanceRequest {
    background_opacity: f64,
    always_on_top: bool,
    compact: bool,
}

impl<'de> Deserialize<'de> for SetTranscriptWindowAppearanceRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawSetTranscriptWindowAppearanceRequest::deserialize(deserializer)?;
        let request = Self {
            background_opacity: raw.background_opacity,
            always_on_top: raw.always_on_top,
            compact: raw.compact,
        };
        request.validate().map_err(D::Error::custom)?;
        Ok(request)
    }
}

impl SetTranscriptWindowAppearanceRequest {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if !valid_opacity(self.background_opacity) {
            return Err("window_appearance_invalid");
        }
        Ok(())
    }

    /// Quantizes to whole percentage points so stored and emitted values are
    /// canonical regardless of slider precision.
    pub(crate) fn into_appearance(self) -> TranscriptWindowAppearance {
        TranscriptWindowAppearance {
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
pub(crate) struct TranscriptWindowGeometry {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawTranscriptWindowGeometry {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl<'de> Deserialize<'de> for TranscriptWindowGeometry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawTranscriptWindowGeometry::deserialize(deserializer)?;
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

impl TranscriptWindowGeometry {
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

/// Everything remembered about the transcript window between runs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TranscriptWindowState {
    pub(crate) schema_version: u8,
    pub(crate) appearance: TranscriptWindowAppearance,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) geometry: Option<TranscriptWindowGeometry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawTranscriptWindowState {
    schema_version: u8,
    appearance: TranscriptWindowAppearance,
    #[serde(default, deserialize_with = "deserialize_optional_non_null")]
    geometry: Option<TranscriptWindowGeometry>,
}

impl<'de> Deserialize<'de> for TranscriptWindowState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawTranscriptWindowState::deserialize(deserializer)?;
        let state = Self {
            schema_version: raw.schema_version,
            appearance: raw.appearance,
            geometry: raw.geometry,
        };
        state.validate().map_err(D::Error::custom)?;
        Ok(state)
    }
}

impl TranscriptWindowState {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1 {
            return Err("window_state_invalid");
        }
        self.appearance.validate()?;
        if let Some(geometry) = self.geometry {
            geometry.validate()?;
        }
        Ok(())
    }
}

impl Default for TranscriptWindowState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            appearance: TranscriptWindowAppearance::default(),
            geometry: None,
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
        let appearance = TranscriptWindowAppearance::default();

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
                serde_json::from_value::<SetTranscriptWindowAppearanceRequest>(request).is_err(),
                "opacity {opacity} must be rejected"
            );
        }
    }

    #[test]
    fn accepted_requests_quantize_to_whole_percentage_points() {
        let request: SetTranscriptWindowAppearanceRequest = serde_json::from_value(json!({
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
        let valid: TranscriptWindowGeometry =
            serde_json::from_value(json!({"x": -1_920, "y": 40, "width": 520, "height": 720}))
                .unwrap();
        valid.validate().unwrap();

        for geometry in [
            json!({"x": 0, "y": 0, "width": 10, "height": 720}),
            json!({"x": 0, "y": 0, "width": 520, "height": 99_999}),
            json!({"x": 999_999, "y": 0, "width": 520, "height": 720}),
        ] {
            assert!(serde_json::from_value::<TranscriptWindowGeometry>(geometry).is_err());
        }
    }

    #[test]
    fn a_window_is_reachable_only_when_enough_of_it_overlaps_a_monitor() {
        let geometry = TranscriptWindowGeometry {
            x: 100,
            y: 100,
            width: 520,
            height: 720,
        };

        assert!(geometry.is_reachable_on(0, 0, 1_920, 1_080));
        // Fully off to the right of the only remaining monitor.
        assert!(!geometry.is_reachable_on(2_000, 0, 1_920, 1_080));
        // A monitor that was unplugged leaves the window on a negative origin.
        let detached = TranscriptWindowGeometry {
            x: -1_900,
            y: 100,
            width: 520,
            height: 720,
        };
        assert!(!detached.is_reachable_on(0, 0, 1_920, 1_080));
        assert!(detached.is_reachable_on(-1_920, 0, 1_920, 1_080));
        // Only a sliver visible is treated as unreachable.
        let sliver = TranscriptWindowGeometry {
            x: 1_900,
            y: 100,
            width: 520,
            height: 720,
        };
        assert!(!sliver.is_reachable_on(0, 0, 1_920, 1_080));
    }

    #[test]
    fn persisted_state_round_trips_and_rejects_invalid_members() {
        let state: TranscriptWindowState = serde_json::from_value(json!({
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

        let restored: TranscriptWindowState =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        assert_eq!(restored, state);

        let without_geometry: TranscriptWindowState = serde_json::from_value(json!({
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
        assert!(serde_json::from_value::<TranscriptWindowState>(unreadable).is_err());
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let request = json!({
            "backgroundOpacity": 0.8,
            "alwaysOnTop": false,
            "compact": false,
            "clickThrough": true
        });
        assert!(serde_json::from_value::<SetTranscriptWindowAppearanceRequest>(request).is_err());
    }
}
