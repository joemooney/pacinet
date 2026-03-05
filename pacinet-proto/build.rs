fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_path = "../proto/pacinet.proto";

    println!("cargo:rerun-if-changed={}", proto_path);

    // Use protox (pure-Rust protobuf parser) instead of shelling out to protoc
    let file_descriptors = protox::compile([proto_path], ["../proto"])?;

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_fds(file_descriptors)?;

    Ok(())
}
