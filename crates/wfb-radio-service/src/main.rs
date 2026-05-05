use anyhow::Result;
use clap::Parser;
use wfb_radio_service::{resolve_service_run, ServiceCli};

fn main() -> Result<()> {
    let cli = ServiceCli::parse();
    let _resolved = resolve_service_run(&cli)?;
    anyhow::bail!("wfb-radio-service runtime execution is not wired yet")
}
