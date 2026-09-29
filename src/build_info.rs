//! Provenance for the startup banner, `--version` and generated files: what
//! was built (version, commit, toolchain: baked in by `build.rs`) and
//! where it is running (host, OS, CPU, memory: read at runtime from
//! `/proc` on Linux, "unknown" elsewhere).

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GIT: &str = env!("MACRO_GEN_GIT");
pub const RUSTC: &str = env!("MACRO_GEN_RUSTC");
pub const TARGET: &str = env!("MACRO_GEN_TARGET");
pub const PROFILE: &str = env!("MACRO_GEN_PROFILE");
pub const NGSPICE: &str = env!("MACRO_GEN_NGSPICE");
const BUILD_EPOCH: &str = env!("MACRO_GEN_BUILD_EPOCH");

/// Cargo features compiled in.
pub fn features() -> &'static str {
    if cfg!(feature = "circt") { "circt" } else { "none" }
}

/// `macro_gen 0.1.0 (1a2b3c4d5e)`: stamped into every generated file.
pub fn generator() -> String {
    format!("macro_gen {VERSION} ({GIT})")
}

pub fn build_time() -> String {
    BUILD_EPOCH.parse().map(utc).unwrap_or_else(|_| "unknown".into())
}

/// The long `--version` text.
pub fn long_version() -> String {
    format!(
        "{VERSION}\ncommit:   {GIT}\nbuilt:    {}\nprofile:  {PROFILE}\ntarget:   {TARGET}\nrustc:    {RUSTC}\nfeatures: {}\nngspice:  {NGSPICE}",
        build_time(),
        features()
    )
}

/// `YYYY-MM-DD HH:MM:SS UTC` for seconds since the Unix epoch.
pub fn utc(epoch_secs: u64) -> String {
    let days = (epoch_secs / 86_400) as i64;
    let secs = epoch_secs % 86_400;
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

pub fn now_utc() -> String {
    utc(SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()))
}

fn read_trimmed(path: &str) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Where this run is happening.
pub struct SystemInfo {
    pub host: String,
    /// e.g. `Ubuntu 25.10, Linux 6.17.0-41-generic (x86_64)`.
    pub os: String,
    /// e.g. `AMD Ryzen 7 7840U (16 threads)`.
    pub cpu: String,
    pub memory: String,
}

impl SystemInfo {
    pub fn collect() -> Self {
        let unknown = || "unknown".to_string();
        let distro = fs::read_to_string("/etc/os-release").ok().and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("PRETTY_NAME="))
                .map(|v| v.trim_matches('"').to_string())
        });
        let kernel = read_trimmed("/proc/sys/kernel/osrelease");
        let os = match (distro, kernel) {
            (Some(d), Some(k)) => format!("{d}, {} {k}", capitalize(std::env::consts::OS)),
            (Some(d), None) => d,
            (None, Some(k)) => format!("{} {k}", capitalize(std::env::consts::OS)),
            (None, None) => capitalize(std::env::consts::OS),
        };
        let model = fs::read_to_string("/proc/cpuinfo").ok().and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("model name"))
                .and_then(|l| l.split_once(':'))
                .map(|(_, v)| v.trim().to_string())
        });
        let threads = std::thread::available_parallelism().map_or(0, |n| n.get());
        let memory = fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find_map(|l| l.strip_prefix("MemTotal:"))
                    .and_then(|v| v.split_whitespace().next()?.parse::<f64>().ok())
            })
            .map_or_else(unknown, |kb| format!("{:.1} GiB", kb / 1024.0 / 1024.0));
        SystemInfo {
            host: read_trimmed("/proc/sys/kernel/hostname").unwrap_or_else(unknown),
            os: format!("{os} ({})", std::env::consts::ARCH),
            cpu: format!("{} ({threads} threads)", model.unwrap_or_else(unknown)),
            memory,
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_formats_known_instants() {
        assert_eq!(utc(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(utc(951_782_400), "2000-02-29 00:00:00 UTC");
        assert_eq!(utc(1_790_000_000), "2026-09-21 14:13:20 UTC");
    }
}
