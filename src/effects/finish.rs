use serde::{Deserialize, Serialize};

use crate::effects::utils::sane;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FinishParams {
    pub enabled: bool,
    pub legacy_grain_amount: f32,
    pub legacy_grain_size: f32,
    pub haze_strength: f32,
    pub lens_distortion: f32,
}

impl Default for FinishParams {
    fn default() -> Self {
        Self {
            enabled: false,
            legacy_grain_amount: 0.0,
            legacy_grain_size: 1.0,
            haze_strength: 0.0,
            lens_distortion: 0.0,
        }
    }
}

impl FinishParams {
    pub fn validated(self) -> Self {
        let d = Self::default();
        Self {
            enabled: self.enabled,
            legacy_grain_amount: sane(self.legacy_grain_amount, 0.0, 0.5, d.legacy_grain_amount),
            legacy_grain_size: sane(self.legacy_grain_size, 1.0, 16.0, d.legacy_grain_size),
            haze_strength: sane(self.haze_strength, 0.0, 4.0, d.haze_strength),
            lens_distortion: sane(self.lens_distortion, -1.0, 1.0, d.lens_distortion),
        }
    }

    pub fn has_any_effect(self) -> bool {
        self.enabled
            && (self.legacy_grain_amount > 1e-6
                || self.haze_strength > 1e-6
                || self.lens_distortion.abs() > 1e-6)
    }
}

#[cfg(test)]
mod tests {
    use super::FinishParams;

    #[test]
    fn finish_validation_clamps_values() {
        let params = FinishParams {
            enabled: true,
            legacy_grain_amount: f32::INFINITY,
            legacy_grain_size: f32::NAN,
            haze_strength: -10.0,
            lens_distortion: 50.0,
        }
        .validated();

        assert!((0.0..=0.5).contains(&params.legacy_grain_amount));
        assert!((1.0..=16.0).contains(&params.legacy_grain_size));
        assert_eq!(params.haze_strength, 0.0);
        assert_eq!(params.lens_distortion, 1.0);
    }
}
