// SPDX-License-Identifier: GPL-3.0-or-later
// afp-catalog on its own (the NAS's Docker image builds only this crate); the player's is vs-gyroflow's, the same
fn main() -> std::process::ExitCode { afp_catalog::cli::main() }
