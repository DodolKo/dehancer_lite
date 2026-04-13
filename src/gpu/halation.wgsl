struct HalationParams {
    threshold: f32,
    knee: f32,
    intensity: f32,
    soft_clip: f32,
    core_weight: f32,
    tail_weight: f32,
    optical_depth: f32,
    _pad0: f32,
    absorption: vec4<f32>,
    chroma_bias: vec4<f32>,
    finish: vec4<f32>,
};

struct BlurParams {
    direction: vec2<f32>,
    radius: f32,
    sigma: f32,
};

@group(0) @binding(0) var source_tex: texture_2d<f32>;
@group(0) @binding(1) var extract_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(2) var<uniform> params: HalationParams;

@group(0) @binding(0) var blur_in: texture_2d<f32>;
@group(0) @binding(1) var blur_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(2) var<uniform> blur: BlurParams;

@group(0) @binding(0) var comp_source: texture_2d<f32>;
@group(0) @binding(1) var comp_core: texture_2d<f32>;
@group(0) @binding(2) var comp_tail: texture_2d<f32>;
@group(0) @binding(3) var comp_out: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(4) var<uniform> comp_params: HalationParams;

fn srgb_to_linear_channel(value: f32) -> f32 {
    if (value <= 0.04045) {
        return value / 12.92;
    }
    return pow((value + 0.055) / 1.055, 2.4);
}

fn linear_to_srgb_channel(value: f32) -> f32 {
    let clamped = max(value, 0.0);
    if (clamped <= 0.0031308) {
        return clamp(12.92 * clamped, 0.0, 1.0);
    }
    return clamp(1.055 * pow(clamped, 1.0 / 2.4) - 0.055, 0.0, 1.0);
}

fn linear_to_aces(linear: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        0.6130973 * linear.r + 0.3395231 * linear.g + 0.0473796 * linear.b,
        0.0701942 * linear.r + 0.9163539 * linear.g + 0.0134519 * linear.b,
        0.0206156 * linear.r + 0.1095698 * linear.g + 0.8698146 * linear.b,
    );
}

fn aces_to_linear(aces: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        1.7050515 * aces.r - 0.6217907 * aces.g - 0.0832584 * aces.b,
        -0.1302571 * aces.r + 1.1408028 * aces.g - 0.0105485 * aces.b,
        -0.0240033 * aces.r - 0.1289688 * aces.g + 1.1529716 * aces.b,
    );
}

fn highlight_mask(luma: f32, threshold: f32, knee: f32) -> f32 {
    if (knee <= 0.000001) {
        return select(0.0, 1.0, luma > threshold);
    }

    let edge0 = threshold - knee;
    let edge1 = threshold + knee;
    let t = clamp((luma - edge0) / max(edge1 - edge0, 0.000001), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn dispatch_bounds_check(pixel: vec2<u32>, size: vec2<u32>) -> bool {
    return pixel.x >= size.x || pixel.y >= size.y;
}

fn distorted_pixel(pixel: vec2<u32>, size: vec2<u32>, distortion: f32) -> vec2<i32> {
    if (abs(distortion) <= 0.000001) {
        return vec2<i32>(pixel);
    }

    let dims = vec2<f32>(max(f32(size.x), 1.0), max(f32(size.y), 1.0));
    let aspect = dims.x / dims.y;
    let uv = (vec2<f32>(pixel) + vec2<f32>(0.5)) / dims;
    let centered = uv * 2.0 - vec2<f32>(1.0);
    let corrected = vec2<f32>(centered.x * aspect, centered.y);
    let r2 = dot(corrected, corrected);
    let factor = 1.0 + distortion * r2;
    let sample_centered = vec2<f32>((corrected.x * factor) / aspect, corrected.y * factor);
    let sample_uv = (sample_centered + vec2<f32>(1.0)) * 0.5;
    let sample_pos = sample_uv * dims - vec2<f32>(0.5);
    let max_coords = vec2<i32>(size) - vec2<i32>(1, 1);

    return clamp(vec2<i32>(round(sample_pos)), vec2<i32>(0, 0), max_coords);
}

fn grain_hash(cell: vec2<f32>, channel: f32) -> f32 {
    let p = cell + vec2<f32>(channel * 19.19, channel * 37.37);
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453) * 2.0 - 1.0;
}

@compute @workgroup_size(8, 8, 1)
fn extract_highlights(@builtin(global_invocation_id) gid: vec3<u32>) {
    let size = textureDimensions(source_tex);
    let pixel = gid.xy;
    if (dispatch_bounds_check(pixel, size)) {
        return;
    }

    let p = vec2<i32>(pixel);
    let srgb = textureLoad(source_tex, p, 0).rgb;
    let linear = vec3<f32>(
        srgb_to_linear_channel(srgb.r),
        srgb_to_linear_channel(srgb.g),
        srgb_to_linear_channel(srgb.b),
    );
    let aces = linear_to_aces(linear);

    let luma = dot(aces, vec3<f32>(0.27222872, 0.67408174, 0.053689517));
    let mask = highlight_mask(luma, params.threshold, params.knee);

    let attenuation = exp(-params.absorption.rgb * params.optical_depth);
    let chroma = max(vec3<f32>(0.0), vec3<f32>(1.0) + params.chroma_bias.rgb);

    let extracted = aces * mask * attenuation * chroma;
    textureStore(extract_out, p, vec4<f32>(extracted, 1.0));
}

@compute @workgroup_size(8, 8, 1)
fn blur_pass(@builtin(global_invocation_id) gid: vec3<u32>) {
    let size = textureDimensions(blur_in);
    let pixel = gid.xy;
    if (dispatch_bounds_check(pixel, size)) {
        return;
    }

    let p = vec2<i32>(pixel);
    let radius = i32(round(blur.radius));
    if (radius <= 0) {
        let c = textureLoad(blur_in, p, 0);
        textureStore(blur_out, p, c);
        return;
    }

    var accum = vec3<f32>(0.0);
    var weight_sum = 0.0;
    let sigma_sq = max(blur.sigma * blur.sigma, 0.0001);
    let max_coords = vec2<i32>(size) - vec2<i32>(1, 1);

    for (var i: i32 = -64; i <= 64; i = i + 1) {
        if (i < -radius || i > radius) {
            continue;
        }

        let delta = vec2<i32>(
            i32(round(f32(i) * blur.direction.x)),
            i32(round(f32(i) * blur.direction.y)),
        );
        let sample_pos = clamp(p + delta, vec2<i32>(0, 0), max_coords);
        let sample_color = textureLoad(blur_in, sample_pos, 0).rgb;
        let weight = exp(-0.5 * f32(i * i) / sigma_sq);

        accum = accum + sample_color * weight;
        weight_sum = weight_sum + weight;
    }

    let blurred = accum / max(weight_sum, 0.000001);
    textureStore(blur_out, p, vec4<f32>(blurred, 1.0));
}

@compute @workgroup_size(8, 8, 1)
fn composite_pass(@builtin(global_invocation_id) gid: vec3<u32>) {
    let size = textureDimensions(comp_source);
    let pixel = gid.xy;
    if (dispatch_bounds_check(pixel, size)) {
        return;
    }

    let p = vec2<i32>(pixel);
    let sample_p = distorted_pixel(pixel, size, comp_params.finish.w);

    let srgb = textureLoad(comp_source, sample_p, 0).rgb;
    let linear = vec3<f32>(
        srgb_to_linear_channel(srgb.r),
        srgb_to_linear_channel(srgb.g),
        srgb_to_linear_channel(srgb.b),
    );
    let base = linear_to_aces(linear);

    let core = textureLoad(comp_core, sample_p, 0).rgb;
    let tail = textureLoad(comp_tail, sample_p, 0).rgb;

    let halation =
        (core * comp_params.core_weight + tail * comp_params.tail_weight) * comp_params.intensity;

    let haze_luma = dot(tail, vec3<f32>(0.27222872, 0.67408174, 0.053689517));
    let haze = haze_luma * comp_params.finish.z * vec3<f32>(1.05, 1.0, 0.92);

    var combined = base + halation + haze;
    combined = combined / (vec3<f32>(1.0) + comp_params.soft_clip * max(combined, vec3<f32>(0.0)));

    let out_linear = aces_to_linear(combined);
    var out_srgb = vec3<f32>(
        linear_to_srgb_channel(out_linear.r),
        linear_to_srgb_channel(out_linear.g),
        linear_to_srgb_channel(out_linear.b),
    );

    let grain_amount = comp_params.finish.x;
    if (grain_amount > 0.000001) {
        let grain_size = max(comp_params.finish.y, 1.0);
        let grain_cell = floor(vec2<f32>(pixel) / grain_size);
        let luma = dot(out_srgb, vec3<f32>(0.2126, 0.7152, 0.0722));
        let grain_amp = grain_amount * (0.35 + 0.65 * clamp(1.0 - luma, 0.0, 1.0));
        let grain = vec3<f32>(
            grain_hash(grain_cell, 0.0),
            grain_hash(grain_cell, 1.0),
            grain_hash(grain_cell, 2.0),
        );
        out_srgb = clamp(out_srgb + grain * grain_amp, vec3<f32>(0.0), vec3<f32>(1.0));
    }

    textureStore(comp_out, p, vec4<f32>(out_srgb, 1.0));
}
