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
