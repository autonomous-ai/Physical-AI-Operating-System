pub fn room_recorder(arguments: &[String]) -> Result<i32> {
    let value = |flag: &str| {
        arguments
            .iter()
            .position(|a| a == flag)
            .and_then(|i| arguments.get(i + 1))
            .cloned()
            .ok_or_else(|| invalid(&format!("fake-room-recorder needs {flag}")))
    };
    let optional = |flag: &str| value(flag).ok();
    let out = PathBuf::from(value("--out")?);
    let seconds: u64 = value("--seconds")?.parse()?;
    let fail_after_ms: u64 = value("--fail-after-ms")?.parse()?;
    // Like `open -W`, a wrapper may exit 0 whatever was recorded.
    let exit_code: i32 = optional("--exit-code").map_or(Ok(1), |v| v.parse())?;
    // A declared capture end, as the observer's ready receipt states it.
    let declared_end = optional("--declare-end-after-ms")
        .map(|ms| ms.parse::<u64>())
        .transpose()?
        .map(|ms| lamp_ipc::monotonic_ns() + ms * 1_000_000);
    fs::DirBuilder::new().mode(0o700).create(&out)?;
    fs::write(
        out.join("ready.json"),
        json!({"fake": true, "note": "fake recorder; records nothing",
            "planned_capture_end_host_monotonic_ns": declared_end})
        .to_string(),
    )?;
    thread::sleep(Duration::from_millis(fail_after_ms.min(seconds * 1000)));
    Ok(exit_code)
}
