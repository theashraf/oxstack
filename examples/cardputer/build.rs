fn main() {
    println!("cargo:rustc-link-arg=-Tdefmt.x");
    // esp-hal's `linkall.x` must be the last linker script.
    println!("cargo:rustc-link-arg=-Tlinkall.x");
    wifi_credentials();
}

/// Turns the git-ignored `wifi.toml` (see `wifi.toml.example`) into
/// compile-time environment variables for the networking examples.
///
/// Format: one `key = "value"` per line; `#` starts a comment line (no
/// inline comments). A value is everything between its first and last
/// quote, taken verbatim (no escape sequences).
fn wifi_credentials() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("wifi.toml");
    let Ok(text) = std::fs::read_to_string(&path) else {
        // Watch the directory so creating the file triggers a rebuild.
        println!(
            "cargo:rerun-if-changed={}",
            path.parent().unwrap().display()
        );
        return;
    };
    println!("cargo:rerun-if-changed={}", path.display());
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            println!("cargo:warning=wifi.toml: ignoring line without `=`");
            continue;
        };
        let value = value.trim();
        let value = match (value.find('"'), value.rfind('"')) {
            (Some(a), Some(b)) if a < b => &value[a + 1..b],
            _ => value,
        };
        let var = match key.trim() {
            "ssid" => "OXSTACK_WIFI_SSID",
            "password" => "OXSTACK_WIFI_PASSWORD",
            "utc_offset_minutes" => "OXSTACK_UTC_OFFSET_MINUTES",
            other => {
                println!("cargo:warning=wifi.toml: unknown key `{other}`");
                continue;
            }
        };
        println!("cargo:rustc-env={var}={value}");
    }
}
