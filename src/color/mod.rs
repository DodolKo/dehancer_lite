//! Color transforms for scene-referred processing.

/// Convert one sRGB channel to linear-light.
pub fn srgb_to_linear(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// Convert one linear-light channel to sRGB.
pub fn linear_to_srgb(value: f32) -> f32 {
    let value = value.max(0.0);
    if value <= 0.003_130_8 {
        (12.92 * value).clamp(0.0, 1.0)
    } else {
        (1.055 * value.powf(1.0 / 2.4) - 0.055).clamp(0.0, 1.0)
    }
}

/// Convert linear sRGB to ACEScg (AP1 primaries).
pub fn linear_to_acescg(rgb: [f32; 3]) -> [f32; 3] {
    // AP0/AP1 conversion constants from ACES reference transforms.
    [
        0.613_097_3 * rgb[0] + 0.339_523_1 * rgb[1] + 0.047_379_6 * rgb[2],
        0.070_194_2 * rgb[0] + 0.916_353_9 * rgb[1] + 0.013_451_9 * rgb[2],
        0.020_615_6 * rgb[0] + 0.109_569_8 * rgb[1] + 0.869_814_6 * rgb[2],
    ]
}

/// Convert ACEScg (AP1 primaries) to linear sRGB.
pub fn acescg_to_linear(aces: [f32; 3]) -> [f32; 3] {
    [
        1.705_051_5 * aces[0] - 0.621_790_7 * aces[1] - 0.083_258_4 * aces[2],
        -0.130_257_1 * aces[0] + 1.140_802_8 * aces[1] - 0.010_548_5 * aces[2],
        -0.024_003_3 * aces[0] - 0.128_968_8 * aces[1] + 1.152_971_6 * aces[2],
    ]
}

#[cfg(test)]
mod tests {
    use approx::assert_relative_eq;

    use super::{acescg_to_linear, linear_to_acescg, linear_to_srgb, srgb_to_linear};

    #[test]
    fn srgb_linear_roundtrip() {
        for sample in [0.0_f32, 0.003, 0.02, 0.18, 0.5, 0.75, 1.0] {
            let linear = srgb_to_linear(sample);
            let srgb = linear_to_srgb(linear);
            assert_relative_eq!(sample, srgb, epsilon = 1e-5);
        }
    }

    #[test]
    fn acescg_linear_roundtrip() {
        let samples = [
            [0.0_f32, 0.0, 0.0],
            [0.18, 0.18, 0.18],
            [0.9, 0.3, 0.1],
            [0.2, 0.8, 0.5],
        ];

        for sample in samples {
            let aces = linear_to_acescg(sample);
            let linear = acescg_to_linear(aces);
            assert_relative_eq!(sample[0], linear[0], epsilon = 1e-4);
            assert_relative_eq!(sample[1], linear[1], epsilon = 1e-4);
            assert_relative_eq!(sample[2], linear[2], epsilon = 1e-4);
        }
    }
}
