//! Validates the WGSL shaders at test time. On the web, WebGPU reports shader
//! errors asynchronously and the page just stays blank, so catch them here.

#[test]
fn terrain_shader_is_valid() {
    let source = include_str!("terrain.wgsl");
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|e| panic!("{}", e.emit_to_string(source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap_or_else(|e| panic!("{}", e.emit_to_string(source)));
}
