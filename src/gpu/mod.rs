pub mod pipeline;

pub use pipeline::GpuHalationProcessor;

#[cfg(test)]
mod tests {
    #[test]
    fn halation_shader_parses() {
        naga::front::wgsl::parse_str(include_str!("halation.wgsl"))
            .expect("halation WGSL shader should parse");
    }
}
