fn main() -> std::io::Result<()> {
    tonic_prost_build::configure().compile_protos(
        &[
            "proto/portal/internal/v1/jobs.proto",
            "proto/portal/internal/v1/query.proto",
        ],
        &["proto"],
    )
}
