// Autogenerate source for sim-core

use serde::{Deserialize, Serialize};
use std::{env, fs::File, io::Write, path::Path, process::Command};

pub fn main() -> () {
    generate_git_version();
    println!("cargo::rerun-if-changed=simh-autoconf.toml");
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename = "autoconf")]
struct AutoConfigData {
    git: GitVersion,
    simh_version: SimhVersion,
}

impl AutoConfigData {
    fn new(git: GitVersion, simh_version: SimhVersion) -> Self {
        AutoConfigData { git, simh_version }
    }
}

impl Default for AutoConfigData {
    fn default() -> Self {
        AutoConfigData::new(GitVersion::default(), SimhVersion::default())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename = "git")]
struct GitVersion {
    #[serde(rename = "hash")]
    git_hash: String,

    #[serde(rename = "timestamp")]
    git_timestamp: String,
}

impl GitVersion {
    fn new(git_hash: String, git_timestamp: String) -> Self {
        GitVersion {
            git_hash,
            git_timestamp,
        }
    }
}

impl Default for GitVersion {
    fn default() -> Self {
        GitVersion::new(String::from("(no git hash)"), String::from("unknown"))
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename = "simh-version")]
struct SimhVersion {
    major: u32,
    minor: u32,
    patch: u32,
    mode: String,
}

impl SimhVersion {
    fn new(major: u32, minor: u32, patch: u32, mode: String) -> Self {
        SimhVersion {
            major,
            minor,
            patch,
            mode,
        }
    }
}

impl Default for SimhVersion {
    fn default() -> Self {
        SimhVersion::new(0, 1, 0, "Development".to_string())
    }
}

const SIMH_AUTOCONF_TOML: &str = "simh-autoconf.toml";
const SRC_VERSION_RS: &str = "src/version.rs";

fn generate_git_version() -> () {
    // Read the release.toml file to get base version information.
    let mut autoconf_toml: AutoConfigData = match std::fs::read_to_string(SIMH_AUTOCONF_TOML) {
        Ok(content) => {
            eprintln!("Read simh-autoconf.toml from src.");
            toml::from_str(&content).unwrap_or(AutoConfigData::default())
        }

        // Default to an empty table if the file cannot be read.
        Err(_e) => {
            eprintln!("Using default AutoConfigData");
            AutoConfigData::default()
        }
    };

    let git_available = Command::new("git").args(&["--version"]).output();

    match git_available {
        Ok(_) => {
            // We know that git is available and we can unwrap values (relatively)
            // safely. Get the commit hash, timestamp and commit state.
            let git_hash_cmd = Command::new("git")
                .args(&["log", "-1", "--pretty=%h"])
                .output()
                .unwrap();
            let git_hash = str::from_utf8(&git_hash_cmd.stdout).unwrap_or("").trim();

            let git_timestamp_cmd = Command::new("git")
                .args(&["log", "-1", "--pretty=%aI", "--date=iso"])
                .output()
                .unwrap();
            let git_timestamp = str::from_utf8(&git_timestamp_cmd.stdout)
                .unwrap_or("")
                .trim()
                .replace("T", " ");

            let has_uncommitted = !Command::new("git")
                .args(&["update-index", "--really-refresh", "--no-verbose"])
                .status()
                .unwrap_or_default()
                .success();

            let git_version_changed = (git_hash != autoconf_toml.git.git_hash.as_str())
                || (git_timestamp != autoconf_toml.git.git_timestamp.as_str());

            if git_version_changed || has_uncommitted || !std::fs::exists(SRC_VERSION_RS).unwrap_or(false) {
                autoconf_toml.git.git_hash = git_hash.to_string();
                autoconf_toml.git.git_timestamp = git_timestamp.to_string();

                write_src_version_rs(&autoconf_toml, has_uncommitted);
                eprintln!("Updated {} with new git version information.", SRC_VERSION_RS);
            }
        }
        Err(e) => {
            eprintln!("Using default version information; git not found: {}", e);

            let mut simh_autoconf_file = File::create(SIMH_AUTOCONF_TOML).unwrap();
            let default_data = AutoConfigData::default();

            std::io::Write::write_all(
                &mut simh_autoconf_file,
                toml::to_string(&AutoConfigData::default()).unwrap().as_bytes(),
            )
            .unwrap();
            eprintln!("Updated {} with default git information.", SIMH_AUTOCONF_TOML);

            write_src_version_rs(&default_data, false);
            eprintln!("Updated {} with default version data.", SRC_VERSION_RS);
        }
    }
}

fn write_src_version_rs(config_data: &AutoConfigData, dirty: bool) -> () {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let version_file_path = Path::new(&out_dir).join("version.rs");
    let mut version_file = File::create(version_file_path).unwrap();

    let version_content = format!(
        r#"// SPDX-License-Identifier: MIT
// Autogenerated by build.rs - do not edit directly.

mod version {{
    pub const SIMH_GIT_HASH: &str = "{}{}";
    pub const _SIMH_GIT_TIMESTAMP: &str = "{}";
    pub const SIMH_VERSION_MAJOR: u32 = {};
    pub const SIMH_VERSION_MINOR: u32 = {};
    pub const SIMH_VERSION_PATCH: u32 = {};
    pub const SIMH_VERSION_MODE: &str = "{}";
}}
"#,
        config_data.git.git_hash,
        if dirty { "+uncommitted" } else { "" },
        config_data.git.git_timestamp,
        config_data.simh_version.major,
        config_data.simh_version.minor,
        config_data.simh_version.patch,
        config_data.simh_version.mode,
    );

    Write::write_all(&mut version_file, version_content.as_bytes()).unwrap();
}
