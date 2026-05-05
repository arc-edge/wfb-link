use anyhow::Result;
use clap::Parser;
use serde::Serialize;
use std::{fs, path::Path};
use wfb_radio_runtime::ProductionRuntimeFlowResult;
use wfb_radio_service::{run_service, ServiceCli};

fn main() -> Result<()> {
    let cli = ServiceCli::parse();
    let emit_json = cli.json;
    let report_path = cli.report.clone();
    let report = run_service(&cli)?;
    emit_report(&report, emit_json, report_path.as_deref())?;
    if !emit_json {
        print_service_report_human(&report);
    }
    if report.result == ProductionRuntimeFlowResult::Fail {
        std::process::exit(1);
    }
    Ok(())
}

fn emit_report<T: Serialize>(
    report: &T,
    emit_json: bool,
    report_path: Option<&Path>,
) -> Result<()> {
    let json = serde_json::to_string_pretty(report)?;
    if emit_json {
        println!("{json}");
    }
    if let Some(path) = report_path {
        fs::write(path, json)?;
    }
    Ok(())
}

fn print_service_report_human(report: &wfb_radio_runtime::ProductionRuntimeFlowReport) {
    println!("wfb-radio-service: {:?}", report.result);
    println!(
        "Channel: {} bandwidth={}MHz stop_reason={}",
        report
            .channel
            .map(|channel| channel.number.to_string())
            .unwrap_or_else(|| "n/a".to_string()),
        report.bandwidth.mhz(),
        report.stop_reason
    );
    println!(
        "Init: {:?} {}/{} phases",
        report.init.readiness, report.init.completed_phase_count, report.init.phase_count
    );
    println!(
        "RX: buffers={} packets={} forwarded={}",
        report.rx.buffers_read, report.rx.parsed_frames, report.rx.forwarded_payloads
    );
    println!(
        "TX: datagrams={} submitted={} failed={} dropped={}",
        report.tx.datagrams_received,
        report.tx.submitted_frames,
        report.tx.failed_submissions,
        report.tx.dropped_datagrams
    );
    if let Some(error) = &report.error {
        println!("Error: {}: {}", error.code, error.message);
    }
}
