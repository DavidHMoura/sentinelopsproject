fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("sentinelops-agent must be a sub-directory of the workspace root")
        .join("proto/sentinel.proto");
    tonic_build::compile_protos(proto_path)?;
    Ok(())
}
