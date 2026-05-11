// SPDX-License-Identifier: GPL-2.0-only
//
// ScroogeDLP - Data Loss Prevention system
// Copyright (C) 2026 SQTX <sssqtx@gmail.com>
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 2 as
// published by the Free Software Foundation.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_file = "../../proto/scrooge.proto";
    let proto_root = "../../proto";

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&[proto_file], &[proto_root])?;

    println!("cargo:rerun-if-changed={proto_file}");
    Ok(())
}
