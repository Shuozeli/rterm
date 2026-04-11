fn main() {
    let schema_dir = std::path::PathBuf::from("schema");
    let mobile_generated = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("..") // crates/rterm-proto -> crates/
        .join("..") // crates/ -> workspace root
        .join("mobile")
        .join("lib")
        .join("generated");

    // Run gRPC codegen. It writes terminalservice_client.dart to OUT_DIR (build/.../out/).
    // We then post-process it: fix the wrong package: import and copy to mobile/lib/generated/.
    //
    // proto_path="." was supposed to produce package-relative import but actually generates
    // package:./rterm/protocol_generated.dart which is invalid (./ is not valid in package: URIs).
    // The flatc output (rterm_rterm.protocol_generated.dart) lives in mobile/lib/generated/.
    // We fix the import to use a relative path and copy both files to mobile/lib/generated/.
    grpc_build::compile_fbs_dart(&[schema_dir.join("rterm.fbs")], &[&schema_dir], ".").unwrap();

    // Find the generated terminalservice_client.dart in OUT_DIR.
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let generated_client = std::path::PathBuf::from(&out_dir).join("terminalservice_client.dart");

    if generated_client.exists() {
        let content = std::fs::read_to_string(&generated_client).unwrap();

        // Replace the wrong import with a relative path.
        // proto_path="." generates package:./rterm/protocol_generated.dart (invalid ./ in package: URI).
        // Fix it to use a simple relative import that resolves from mobile/lib/generated/.
        let fixed = content.replace(
            "import 'package:./rterm/protocol_generated.dart';",
            "import 'rterm_rterm.protocol_generated.dart';",
        );

        // Copy terminalservice_client.dart to mobile/lib/generated/.
        std::fs::create_dir_all(&mobile_generated).unwrap();
        std::fs::write(mobile_generated.join("terminalservice_client.dart"), &fixed).unwrap();
    }
}
