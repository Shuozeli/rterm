pub fn get_lan_ip() -> Option<String> {
    let output = std::process::Command::new("hostname")
        .arg("-I")
        .output()
        .ok()?;
    let ips = String::from_utf8_lossy(&output.stdout);
    ips.split_whitespace().next().map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_lan_ip_does_not_panic() {
        // Returns Some(ip) on a machine with a LAN address, or None if hostname -I
        // is unavailable. Either outcome is valid; the function must not panic.
        let result = get_lan_ip();
        if let Some(ip) = result {
            assert!(!ip.is_empty(), "returned IP string should not be empty");
        }
    }
}
