fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protos = [
        "proto/lsi/common/v1/common.proto",
        "proto/lsi/daemon/v1/daemon.proto",
        "proto/lsi/peers/v1/peers.proto",
        "proto/lsi/transfers/v1/transfers.proto",
        "proto/lsi/inbox/v1/inbox.proto",
        "proto/lsi/events/v1/events.proto",
    ];

    for proto in protos {
        println!("cargo:rerun-if-changed={proto}");
    }

    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .compile_protos(&protos, &["proto"])?;

    Ok(())
}
