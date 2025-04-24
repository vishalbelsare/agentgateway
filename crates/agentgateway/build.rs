use std::env;

// This build script is used to generate the rust source files that
// we need for XDS GRPC communication.
fn main() -> Result<(), anyhow::Error> {
	// Fuzzing uses custom cfg (https://rust-fuzz.github.io/book/cargo-fuzz/guide.html)
	// Tell cargo to expect this (https://doc.rust-lang.org/nightly/rustc/check-cfg/cargo-specifics.html).
	println!("cargo::rustc-check-cfg=cfg(fuzzing)");
	let proto_files = [
		"proto/a2a/target.proto",
		"proto/mcp/target.proto",
		"proto/xds.proto",
		"proto/common.proto",
		"proto/listener.proto",
		"proto/rbac.proto",
	]
	.iter()
	.map(|name| std::env::current_dir().unwrap().join(name))
	.collect::<Vec<_>>();
	let include_dirs = ["proto/"]
		.iter()
		.map(|i| std::env::current_dir().unwrap().join(i))
		.collect::<Vec<_>>();
	let config = {
		let mut c = prost_build::Config::new();
		c.disable_comments(Some("."));
		c
	};

	let out_dir = env::var("OUT_DIR").unwrap();
	let descriptor_path = std::path::PathBuf::from(out_dir.clone()).join("proto_descriptor.bin");

	tonic_build::configure()
		.build_server(true)
		.file_descriptor_set_path(descriptor_path.clone())
		.compile_well_known_types(true)
		.extern_path(".google.protobuf", "::pbjson_types")
		.compile_protos_with_config(
			config,
			&proto_files
				.iter()
				.map(|path| path.to_str().unwrap())
				.collect::<Vec<_>>(),
			&include_dirs
				.iter()
				.map(|p| p.to_str().unwrap())
				.collect::<Vec<_>>(),
		)?;

	// This tells cargo to re-run this build script only when the proto files
	// we're interested in change or the any of the proto directories were updated.
	for path in [proto_files, include_dirs].concat() {
		println!("cargo:rerun-if-changed={}", path.to_str().unwrap());
	}
	let descriptor_set = std::fs::read(descriptor_path).expect("descriptors not present");
	pbjson_build::Builder::new()
		.register_descriptors(&descriptor_set)?
		.preserve_proto_field_names()
		.emit_fields()
		.build(&[
			".agentgateway.dev.a2a.target",
			".agentgateway.dev.mcp.target",
			".agentgateway.dev.common",
			".agentgateway.dev.listener",
			".agentgateway.dev.rbac",
		])?;

	Ok(())
}
