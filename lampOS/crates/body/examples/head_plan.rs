//! Offline-only preflight. It has no serial open or worker-spawn path.
use lamp_body::runtime::{
    config::{Config, bounded_path},
    qualification::HeadPlan,
};
use std::{
    fs::File,
    io::{self, Read},
    os::unix::fs::MetadataExt,
    path::Path,
};
fn private_plan(path: &Path) -> io::Result<HeadPlan> {
    bounded_path(path)?;
    let fd = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC
            | rustix::fs::OFlags::NONBLOCK,
        rustix::fs::Mode::empty(),
    )?;
    let file = File::from(fd);
    let meta = file.metadata()?;
    if !meta.is_file()
        || meta.nlink() != 1
        || meta.uid() != rustix::process::geteuid().as_raw()
        || meta.mode() & 0o077 != 0
        || meta.len() > 4096
    {
        return Err(io::Error::other(
            "plan must be an owned private regular file <=4096 bytes",
        ));
    }
    let mut data = Vec::new();
    file.take(4097).read_to_end(&mut data)?;
    if data.len() > 4096 {
        return Err(io::Error::other("plan grew beyond bound"));
    }
    serde_json::from_slice(&data).map_err(io::Error::other)
}
fn main() -> io::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).take(4).collect();
    if args.len() != 3 || args[0] != "--dry-run" {
        return Err(io::Error::other(
            "usage: head_plan --dry-run ABS_PRIVATE_WORKER_CONFIG ABS_PRIVATE_HEAD_PLAN; no execute mode",
        ));
    }
    let config = Config::read(Path::new(&args[1]))?;
    let valid = config.validate()?;
    let plan = private_plan(Path::new(&args[2]))?;
    plan.validate(&valid.calibration)?;
    if plan.qualification_record != config.qualification.record
        || plan.readback_age_us > config.timing.readback_age_us
    {
        return Err(io::Error::other(
            "plan and provisioned worker qualification differ",
        ));
    }
    println!(
        "{{\"kind\":\"head_plan_dry_run\",\"valid_structure\":true,\"hardware_opened\":false,\"motion_authorized\":false,\"transmitted_ids\":[5],\"maximum_segments\":2,\"requires_original_live_authority\":true}}"
    );
    Ok(())
}
