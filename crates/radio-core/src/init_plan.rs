use serde::Serialize;

use crate::{FirmwareError, FirmwareImage, UsbTraceEvent, UsbTraceKind};

pub const INIT_DRY_RUN_FIRMWARE_CHUNK_SIZE: usize = MAX_DLFW_PAGE_SIZE;
pub const INIT_PLAN_REFERENCE_REPO: &str = "https://github.com/aircrack-ng/rtl8812au";
pub const INIT_PLAN_REFERENCE_COMMIT: &str = "734485506a30d6237c2deaad666a19f8ca5379f2";

const SRC_USB_HALINIT: &str = "aircrack-ng/rtl8812au@7344855:hal/rtl8812a/usb/usb_halinit.c";
const SRC_HAL_INIT: &str = "aircrack-ng/rtl8812au@7344855:hal/rtl8812a/rtl8812a_hal_init.c";
const SRC_PWRSEQ: &str = "aircrack-ng/rtl8812au@7344855:include/Hal8812PwrSeq.h";
const SRC_REGS: &str = "aircrack-ng/rtl8812au@7344855:include/hal_com_reg.h";
const SRC_PHYCFG: &str = "aircrack-ng/rtl8812au@7344855:hal/rtl8812a/rtl8812a_phycfg.c";

const REG_SYS_FUNC_EN: u16 = 0x0002;
const REG_APS_FSMCO: u16 = 0x0004;
const REG_SYS_CLKR: u16 = 0x0008;
const REG_AFE_XTAL_CTRL: u16 = 0x0024;
const REG_AFE_PLL_CTRL: u16 = 0x0028;
const REG_RF_CTRL: u16 = 0x001f;
const REG_OPT_CTRL_8812: u16 = 0x0074;
const REG_RF_B_CTRL_8812: u16 = REG_OPT_CTRL_8812 + 2;
const REG_MCUFWDL: u16 = 0x0080;
const REG_CR: u16 = 0x0100;
const REG_PBP: u16 = 0x0104;
const REG_TRXFF_BNDY: u16 = 0x0114;
const REG_LLT_INIT: u16 = 0x01e0;
const REG_RQPN: u16 = 0x0200;
const REG_TDECTRL: u16 = 0x0208;
const REG_RQPN_NPQ: u16 = 0x0214;
const REG_RXDMA_AGG_PG_TH: u16 = 0x0280;
const REG_RXDMA_STATUS: u16 = 0x0288;
const REG_RXDMA_PRO_8812: u16 = 0x0290;
const REG_FWHW_TXQ_CTRL: u16 = 0x0420;
const REG_AMPDU_MAX_TIME_8812: u16 = 0x0456;
const REG_AMPDU_MAX_LENGTH_8812: u16 = 0x0458;
const REG_AMPDU_BURST_MODE_8812: u16 = 0x04bc;
const REG_RX_DRVINFO_SZ: u16 = 0x060f;
const REG_RXFLTMAP1: u16 = 0x06a2;
const REG_MAR: u16 = 0x0620;
const REG_RRSR: u16 = 0x0440;
const REG_SPEC_SIFS: u16 = 0x0428;
const REG_RETRY_LIMIT: u16 = 0x042a;
const REG_ACKTO: u16 = 0x0640;
const REG_HWSEQ_CTRL: u16 = 0x0423;
const REG_BAR_MODE_CTRL: u16 = 0x04cc;
const REG_NAV_UPPER: u16 = 0x0652;
const REG_USTIME_TSF: u16 = 0x055c;
const REG_USTIME_EDCA: u16 = 0x0638;
const REG_RX_PKT_LIMIT: u16 = 0x060c;
const REG_PIFS: u16 = 0x0512;
const REG_OFDMCCKEN_JAGUAR: u16 = 0x0808;

const FW_START_ADDRESS: u16 = 0x1000;
const MAX_DLFW_PAGE_SIZE: usize = 4096;
const MAX_REG_BLOCK_SIZE: usize = 196;
const FIRMWARE_REMAINDER_BLOCK_SIZE: usize = 8;
// Mirrors aircrack-ng's CONFIG_BEAMFORMER_FW_NDPA build: 0xff - 7 beacon
// pages - 2 firmware NDPA pages, then boundary is total + 1.
const TX_PAGE_BOUNDARY_8812: u8 = 0xf7;
const LAST_ENTRY_OF_TX_PKT_BUFFER_8812: u8 = 0xff;
const DRVINFO_SZ: usize = 4;
const RX_DMA_BOUNDARY_8812: u16 = 0x3e7f;

const HCI_TXDMA_EN: u16 = 1 << 0;
const HCI_RXDMA_EN: u16 = 1 << 1;
const TXDMA_EN: u16 = 1 << 2;
const RXDMA_EN: u16 = 1 << 3;
const PROTOCOL_EN: u16 = 1 << 4;
const SCHEDULE_EN: u16 = 1 << 5;
const MACTXEN: u8 = 1 << 6;
const MACRXEN: u8 = 1 << 7;
const ENSEC: u16 = 1 << 9;
const CALTMR_EN: u16 = 1 << 10;
const MCUFWDL_EN: u8 = 1 << 0;
const MCUFWDL_RDY: u32 = 1 << 1;
const FWDL_CHKSUM_RPT: u8 = 1 << 2;
const WINTINI_RDY: u32 = 1 << 6;
const RAM_DL_SEL: u8 = 1 << 7;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InitDryRunPlan {
    pub firmware_len: usize,
    pub firmware_chunk_size: usize,
    pub source_repo: &'static str,
    pub source_commit: &'static str,
    pub transfers: Vec<PlannedInitTransfer>,
}

impl InitDryRunPlan {
    pub fn trace_events(&self) -> Vec<UsbTraceEvent> {
        self.transfers
            .iter()
            .map(|transfer| transfer.event.clone())
            .collect()
    }

    pub fn phase_counts(&self) -> Vec<InitPhaseCount> {
        InitPhase::ORDERED
            .iter()
            .map(|phase| InitPhaseCount {
                phase: *phase,
                transfers: self
                    .transfers
                    .iter()
                    .filter(|transfer| transfer.phase == *phase)
                    .count(),
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlannedInitTransfer {
    pub phase: InitPhase,
    pub operation: InitOperation,
    pub register: Option<InitRegister>,
    pub description: &'static str,
    pub source: &'static str,
    pub event: UsbTraceEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InitOperation {
    Read,
    Write,
    Poll,
    FirmwareBlock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct InitRegister {
    pub name: &'static str,
    pub address: u16,
    pub width: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InitPhase {
    Preflight,
    RfReset,
    PowerOn,
    MacReset,
    Llt,
    FirmwareDownload,
    FirmwareChecksumPoll,
    FirmwareReadyPoll,
    MacConfig,
    QueueDma,
    Wmac,
    BbRf,
    Channel,
}

impl InitPhase {
    const ORDERED: &'static [InitPhase] = &[
        InitPhase::Preflight,
        InitPhase::RfReset,
        InitPhase::PowerOn,
        InitPhase::MacReset,
        InitPhase::Llt,
        InitPhase::FirmwareDownload,
        InitPhase::FirmwareChecksumPoll,
        InitPhase::FirmwareReadyPoll,
        InitPhase::MacConfig,
        InitPhase::QueueDma,
        InitPhase::Wmac,
        InitPhase::BbRf,
        InitPhase::Channel,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct InitPhaseCount {
    pub phase: InitPhase,
    pub transfers: usize,
}

pub fn plan_rtl8812au_init(firmware: &FirmwareImage) -> Result<InitDryRunPlan, FirmwareError> {
    let mut transfers = Vec::new();
    let firmware_payload = firmware.realtek_download_payload();
    add_preflight_transfers(&mut transfers);
    add_rf_reset_transfers(&mut transfers);
    add_power_on_transfers(&mut transfers);
    add_mac_reset_transfers(&mut transfers);
    add_llt_transfers(&mut transfers);
    add_firmware_transfers(firmware_payload.bytes, &mut transfers);
    add_mac_config_transfers(&mut transfers);
    add_queue_dma_transfers(&mut transfers);
    add_wmac_transfers(&mut transfers);
    add_bb_rf_transfers(&mut transfers);
    add_channel_transfers(&mut transfers);

    Ok(InitDryRunPlan {
        firmware_len: firmware_payload.bytes.len(),
        firmware_chunk_size: INIT_DRY_RUN_FIRMWARE_CHUNK_SIZE,
        source_repo: INIT_PLAN_REFERENCE_REPO,
        source_commit: INIT_PLAN_REFERENCE_COMMIT,
        transfers,
    })
}

fn add_preflight_transfers(transfers: &mut Vec<PlannedInitTransfer>) {
    read_reg(
        transfers,
        InitPhase::Preflight,
        reg("REG_SYS_CLKR + 1", REG_SYS_CLKR + 1, 1),
        "check MAC clock state before init",
        SRC_USB_HALINIT,
    );
    read_reg(
        transfers,
        InitPhase::Preflight,
        reg("REG_CR", REG_CR, 1),
        "check command register before init",
        SRC_USB_HALINIT,
    );
    read_reg(
        transfers,
        InitPhase::Preflight,
        reg("REG_MCUFWDL", REG_MCUFWDL, 1),
        "check whether RAM firmware is already selected",
        SRC_USB_HALINIT,
    );
}

fn add_rf_reset_transfers(transfers: &mut Vec<PlannedInitTransfer>) {
    write_reg(
        transfers,
        InitPhase::RfReset,
        reg("REG_RF_CTRL", REG_RF_CTRL, 1),
        "reset RF path A before MAC power-on",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::RfReset,
        reg("REG_RF_CTRL", REG_RF_CTRL, 1),
        "release RF path A reset before MAC power-on",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::RfReset,
        reg("REG_RF_B_CTRL_8812", REG_RF_B_CTRL_8812, 1),
        "reset RF path B before MAC power-on",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::RfReset,
        reg("REG_RF_B_CTRL_8812", REG_RF_B_CTRL_8812, 1),
        "release RF path B reset before MAC power-on",
        SRC_USB_HALINIT,
    );
}

fn add_power_on_transfers(transfers: &mut Vec<PlannedInitTransfer>) {
    rmw_reg8(
        transfers,
        InitPhase::PowerOn,
        reg("REG_APS_FSMCO + 1", REG_APS_FSMCO + 1, 1),
        "disable SW low-power state in RTL8812 card-emulation to active flow",
        SRC_PWRSEQ,
    );
    poll_reg(
        transfers,
        InitPhase::PowerOn,
        reg("REG_APS_FSMCO + 2", REG_APS_FSMCO + 2, 1),
        "poll power-ready bit in RTL8812 card-emulation to active flow",
        SRC_PWRSEQ,
    );
    rmw_reg8(
        transfers,
        InitPhase::PowerOn,
        reg("REG_APS_FSMCO + 1", REG_APS_FSMCO + 1, 1),
        "disable WLAN suspend in power sequence",
        SRC_PWRSEQ,
    );
    rmw_reg8(
        transfers,
        InitPhase::PowerOn,
        reg("REG_APS_FSMCO + 1", REG_APS_FSMCO + 1, 1),
        "request MAC power-on transition",
        SRC_PWRSEQ,
    );
    poll_reg(
        transfers,
        InitPhase::PowerOn,
        reg("REG_APS_FSMCO + 1", REG_APS_FSMCO + 1, 1),
        "poll MAC power-on transition completion",
        SRC_PWRSEQ,
    );
    rmw_reg8(
        transfers,
        InitPhase::PowerOn,
        reg("REG_AFE_XTAL_CTRL", REG_AFE_XTAL_CTRL, 1),
        "select post-XOSC buffer type",
        SRC_PWRSEQ,
    );
    rmw_reg8(
        transfers,
        InitPhase::PowerOn,
        reg("REG_AFE_PLL_CTRL", REG_AFE_PLL_CTRL, 1),
        "select post-XOSC PLL buffer type",
        SRC_PWRSEQ,
    );
    write_reg(
        transfers,
        InitPhase::PowerOn,
        reg("REG_CR", REG_CR, 2),
        "clear command register before enabling DMA and scheduler blocks",
        SRC_USB_HALINIT,
    );
    read_reg(
        transfers,
        InitPhase::PowerOn,
        reg("REG_CR", REG_CR, 2),
        "read command register for block-enable update",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::PowerOn,
        reg("REG_CR", REG_CR, 2),
        "enable HCI DMA, TX/RX DMA, protocol, scheduler, security, and calibration timer",
        SRC_USB_HALINIT,
    );
}

fn add_mac_reset_transfers(transfers: &mut Vec<PlannedInitTransfer>) {
    read_reg(
        transfers,
        InitPhase::MacReset,
        reg("REG_MCUFWDL", REG_MCUFWDL, 1),
        "check RAM_DL_SEL before optional 8051/MAC reset",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::MacReset,
        reg("REG_MCUFWDL", REG_MCUFWDL, 1),
        "clear MCU firmware-download state if previous RAM code was active",
        SRC_USB_HALINIT,
    );
    rmw_reg8(
        transfers,
        InitPhase::MacReset,
        reg("REG_SYS_FUNC_EN", REG_SYS_FUNC_EN, 1),
        "reset BB before reinitializing MAC",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::MacReset,
        reg("REG_RF_CTRL", REG_RF_CTRL, 1),
        "reset RF control during MAC reset path",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::MacReset,
        reg("REG_CR", REG_CR, 2),
        "reset TX/RX path during MAC reset path",
        SRC_USB_HALINIT,
    );
    rmw_reg8(
        transfers,
        InitPhase::MacReset,
        reg("REG_APS_FSMCO + 1", REG_APS_FSMCO + 1, 1),
        "turn MAC state machine off during reset",
        SRC_USB_HALINIT,
    );
    poll_reg(
        transfers,
        InitPhase::MacReset,
        reg("REG_APS_FSMCO + 1", REG_APS_FSMCO + 1, 1),
        "poll MAC state machine reset completion",
        SRC_USB_HALINIT,
    );
    rmw_reg8(
        transfers,
        InitPhase::MacReset,
        reg("REG_APS_FSMCO + 1", REG_APS_FSMCO + 1, 1),
        "turn MAC state machine back on after reset",
        SRC_USB_HALINIT,
    );
    rmw_reg8(
        transfers,
        InitPhase::MacReset,
        reg("REG_SYS_FUNC_EN + 1", REG_SYS_FUNC_EN + 1, 1),
        "toggle upper SYS_FUNC_EN bits after reset",
        SRC_USB_HALINIT,
    );
}

fn add_llt_transfers(transfers: &mut Vec<PlannedInitTransfer>) {
    for address in 0..(TX_PAGE_BOUNDARY_8812 - 1) {
        add_llt_write(transfers, address, address + 1);
    }
    add_llt_write(transfers, TX_PAGE_BOUNDARY_8812 - 1, 0xff);
    for address in TX_PAGE_BOUNDARY_8812..LAST_ENTRY_OF_TX_PKT_BUFFER_8812 {
        add_llt_write(transfers, address, address + 1);
    }
    add_llt_write(
        transfers,
        LAST_ENTRY_OF_TX_PKT_BUFFER_8812,
        TX_PAGE_BOUNDARY_8812,
    );
}

fn add_firmware_transfers(firmware_payload: &[u8], transfers: &mut Vec<PlannedInitTransfer>) {
    read_reg(
        transfers,
        InitPhase::FirmwareDownload,
        reg("REG_MCUFWDL", REG_MCUFWDL, 1),
        "check whether 8051 RAM code is already active before firmware download",
        SRC_HAL_INIT,
    );
    write_reg(
        transfers,
        InitPhase::FirmwareDownload,
        reg("REG_MCUFWDL", REG_MCUFWDL, 1),
        "reset MCU firmware-download state before download",
        SRC_HAL_INIT,
    );
    rmw_reg8(
        transfers,
        InitPhase::FirmwareDownload,
        reg("REG_MCUFWDL", REG_MCUFWDL, 1),
        "enable MCU firmware download",
        SRC_HAL_INIT,
    );
    rmw_reg8(
        transfers,
        InitPhase::FirmwareDownload,
        reg("REG_MCUFWDL + 2", REG_MCUFWDL + 2, 1),
        "hold 8051 reset while firmware download is enabled",
        SRC_HAL_INIT,
    );
    rmw_reg8(
        transfers,
        InitPhase::FirmwareDownload,
        reg("REG_MCUFWDL", REG_MCUFWDL, 1),
        "reset firmware-download checksum report bit",
        SRC_HAL_INIT,
    );

    for (page, chunk) in firmware_payload.chunks(MAX_DLFW_PAGE_SIZE).enumerate() {
        read_reg(
            transfers,
            InitPhase::FirmwareDownload,
            reg("REG_MCUFWDL + 2", REG_MCUFWDL + 2, 1),
            "read firmware page selector",
            SRC_HAL_INIT,
        );
        write_reg(
            transfers,
            InitPhase::FirmwareDownload,
            reg("REG_MCUFWDL + 2", REG_MCUFWDL + 2, 1),
            firmware_page_description(page),
            SRC_HAL_INIT,
        );
        add_firmware_page_blocks(transfers, chunk.len());
    }

    for _ in 0..5 {
        poll_reg(
            transfers,
            InitPhase::FirmwareChecksumPoll,
            reg("REG_MCUFWDL", REG_MCUFWDL, 4),
            "poll FWDL_ChkSum_rpt after firmware block writes",
            SRC_HAL_INIT,
        );
    }
    rmw_reg8(
        transfers,
        InitPhase::FirmwareDownload,
        reg("REG_MCUFWDL", REG_MCUFWDL, 1),
        "disable MCU firmware download after checksum report",
        SRC_HAL_INIT,
    );
    read_reg(
        transfers,
        InitPhase::FirmwareReadyPoll,
        reg("REG_MCUFWDL", REG_MCUFWDL, 4),
        "read firmware control before setting MCUFWDL_RDY",
        SRC_HAL_INIT,
    );
    write_reg(
        transfers,
        InitPhase::FirmwareReadyPoll,
        reg("REG_MCUFWDL", REG_MCUFWDL, 4),
        "set MCUFWDL_RDY and clear WINTINI_RDY before 8051 reset",
        SRC_HAL_INIT,
    );
    rmw_reg8(
        transfers,
        InitPhase::FirmwareReadyPoll,
        reg("REG_SYS_FUNC_EN + 1", REG_SYS_FUNC_EN + 1, 1),
        "toggle 8051 reset after firmware download",
        SRC_HAL_INIT,
    );
    for _ in 0..10 {
        poll_reg(
            transfers,
            InitPhase::FirmwareReadyPoll,
            reg("REG_MCUFWDL", REG_MCUFWDL, 4),
            "poll WINTINI_RDY after firmware download",
            SRC_HAL_INIT,
        );
    }
}

fn add_mac_config_transfers(transfers: &mut Vec<PlannedInitTransfer>) {
    read_reg(
        transfers,
        InitPhase::MacConfig,
        reg("PHY MAC table", 0x0000, 0),
        "load PHY_MACConfig8812 table entries from the driver image",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::MacConfig,
        reg("REG_PBP", REG_PBP, 1),
        "set TX packet-buffer page size to 512 bytes",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::MacConfig,
        reg("REG_BCNQ_BDNY", 0x0424, 1),
        "set beacon queue TX buffer boundary",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::MacConfig,
        reg("REG_MGQ_BDNY", 0x0425, 1),
        "set management queue TX buffer boundary",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::MacConfig,
        reg("REG_TRXFF_BNDY", REG_TRXFF_BNDY, 1),
        "set TX/RX FIFO boundary low byte",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::MacConfig,
        reg("REG_TRXFF_BNDY + 2", REG_TRXFF_BNDY + 2, 2),
        "set RX FIFO page boundary",
        SRC_USB_HALINIT,
    );
}

fn add_queue_dma_transfers(transfers: &mut Vec<PlannedInitTransfer>) {
    write_reg(
        transfers,
        InitPhase::QueueDma,
        reg("REG_RQPN_NPQ", REG_RQPN_NPQ, 1),
        "program normal-priority queue reserved pages",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::QueueDma,
        reg("REG_RQPN", REG_RQPN, 4),
        "load TX DMA reserved page numbers",
        SRC_USB_HALINIT,
    );
    read_reg(
        transfers,
        InitPhase::QueueDma,
        reg("REG_TXDMA_OFFSET_CHK", 0x020c, 4),
        "read TX DMA offset check before optional incorrect bulk-out drop bit",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::QueueDma,
        reg("REG_TXDMA_OFFSET_CHK", 0x020c, 4),
        "write TX DMA offset check policy",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::QueueDma,
        reg("REG_RXDMA_STATUS", REG_RXDMA_STATUS, 2),
        "set RX DMA burst length",
        SRC_USB_HALINIT,
    );
    read_reg(
        transfers,
        InitPhase::QueueDma,
        reg("REG_RXDMA_PRO_8812", REG_RXDMA_PRO_8812, 1),
        "read RX DMA burst packet length policy",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::QueueDma,
        reg("REG_RXDMA_PRO_8812", REG_RXDMA_PRO_8812, 1),
        "write RX DMA burst packet length policy",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::QueueDma,
        reg("REG_RXDMA_AGG_PG_TH", REG_RXDMA_AGG_PG_TH, 2),
        "program RX aggregation page threshold",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::QueueDma,
        reg("REG_TDECTRL", REG_TDECTRL, 1),
        "set TX DMA descriptor page policy",
        SRC_USB_HALINIT,
    );
}

fn add_wmac_transfers(transfers: &mut Vec<PlannedInitTransfer>) {
    write_reg(
        transfers,
        InitPhase::Wmac,
        reg("REG_RX_DRVINFO_SZ", REG_RX_DRVINFO_SZ, 1),
        "include RX PHY status driver-info bytes for RSSI metadata",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::Wmac,
        reg("REG_MAR", REG_MAR, 4),
        "accept all multicast address bits low",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::Wmac,
        reg("REG_MAR + 4", REG_MAR + 4, 4),
        "accept all multicast address bits high",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::Wmac,
        reg("REG_RXFLTMAP1", REG_RXFLTMAP1, 2),
        "configure control-frame receive filter map",
        SRC_USB_HALINIT,
    );
    read_reg(
        transfers,
        InitPhase::Wmac,
        reg("REG_RRSR", REG_RRSR, 4),
        "read response-rate set before conservative update",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::Wmac,
        reg("REG_RRSR", REG_RRSR, 4),
        "write conservative response-rate set",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::Wmac,
        reg("REG_SPEC_SIFS", REG_SPEC_SIFS, 2),
        "set SIFS timing",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::Wmac,
        reg("REG_RETRY_LIMIT", REG_RETRY_LIMIT, 2),
        "set short and long retry limits",
        SRC_USB_HALINIT,
    );
    read_reg(
        transfers,
        InitPhase::Wmac,
        reg("REG_FWHW_TXQ_CTRL", REG_FWHW_TXQ_CTRL, 1),
        "read firmware/hardware TX queue control before retry update",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::Wmac,
        reg("REG_FWHW_TXQ_CTRL", REG_FWHW_TXQ_CTRL, 1),
        "enable newer AMPDU retry behavior",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::Wmac,
        reg("REG_ACKTO", REG_ACKTO, 1),
        "set ACK timeout",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::Wmac,
        reg("REG_AMPDU_MAX_TIME_8812", REG_AMPDU_MAX_TIME_8812, 1),
        "set AMPDU maximum time",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::Wmac,
        reg("REG_AMPDU_MAX_LENGTH_8812", REG_AMPDU_MAX_LENGTH_8812, 4),
        "set AMPDU maximum length",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::Wmac,
        reg("REG_USTIME_TSF", REG_USTIME_TSF, 1),
        "set TSF microsecond timing",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::Wmac,
        reg("REG_USTIME_EDCA", REG_USTIME_EDCA, 1),
        "set EDCA microsecond timing",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::Wmac,
        reg("REG_RX_PKT_LIMIT", REG_RX_PKT_LIMIT, 1),
        "set VHT receive packet-length limit",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::Wmac,
        reg("REG_PIFS", REG_PIFS, 1),
        "set PIFS timing",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::Wmac,
        reg("REG_AMPDU_BURST_MODE_8812", REG_AMPDU_BURST_MODE_8812, 1),
        "set AMPDU burst mode when enabled by config",
        SRC_USB_HALINIT,
    );
    read_reg(
        transfers,
        InitPhase::Wmac,
        reg("REG_CR", REG_CR, 1),
        "read command register before enabling MAC TX/RX",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::Wmac,
        reg("REG_CR", REG_CR, 1),
        "enable MAC TX and RX after FIFO boundaries are configured",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::Wmac,
        reg("REG_HWSEQ_CTRL", REG_HWSEQ_CTRL, 1),
        "enable hardware sequence numbers",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::Wmac,
        reg("REG_BAR_MODE_CTRL", REG_BAR_MODE_CTRL, 4),
        "disable BAR behavior per init sequence",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::Wmac,
        reg("REG_NAV_UPPER", REG_NAV_UPPER, 1),
        "apply NAV upper limit",
        SRC_USB_HALINIT,
    );
}

fn add_bb_rf_transfers(transfers: &mut Vec<PlannedInitTransfer>) {
    read_reg(
        transfers,
        InitPhase::BbRf,
        reg("REG_SYS_FUNC_EN", REG_SYS_FUNC_EN, 1),
        "read SYS_FUNC_EN before enabling USBA and BB reset bits",
        SRC_PHYCFG,
    );
    write_reg(
        transfers,
        InitPhase::BbRf,
        reg("REG_SYS_FUNC_EN", REG_SYS_FUNC_EN, 1),
        "enable USB analog path for BB/RF configuration",
        SRC_PHYCFG,
    );
    write_reg(
        transfers,
        InitPhase::BbRf,
        reg("REG_SYS_FUNC_EN", REG_SYS_FUNC_EN, 1),
        "enable BB global reset and BB reset bits",
        SRC_PHYCFG,
    );
    write_reg(
        transfers,
        InitPhase::BbRf,
        reg("REG_RF_CTRL", REG_RF_CTRL, 1),
        "power on RF path A",
        SRC_PHYCFG,
    );
    write_reg(
        transfers,
        InitPhase::BbRf,
        reg("REG_RF_B_CTRL_8812", REG_RF_B_CTRL_8812, 1),
        "power on RF path B",
        SRC_PHYCFG,
    );
    write_reg(
        transfers,
        InitPhase::BbRf,
        reg("PHY BB table", 0x0800, 0),
        "load PHY_BBConfig8812 table entries",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::BbRf,
        reg("PHY RF path A table", 0x0000, 0),
        "load PHY_RFConfig8812 path A table entries",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::BbRf,
        reg("PHY RF path B table", 0x0000, 0),
        "load PHY_RFConfig8812 path B table entries",
        SRC_USB_HALINIT,
    );
}

fn add_channel_transfers(transfers: &mut Vec<PlannedInitTransfer>) {
    write_reg(
        transfers,
        InitPhase::Channel,
        reg("PHY band switch", 0x0000, 0),
        "select 2.4 GHz or 5 GHz band before channel setup",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::Channel,
        reg("rOFDMCCKEN_Jaguar", REG_OFDMCCKEN_JAGUAR, 4),
        "ensure OFDM/CCK BB clocks remain enabled after band selection",
        SRC_PHYCFG,
    );
    write_reg(
        transfers,
        InitPhase::Channel,
        reg("PHY channel/bandwidth", 0x0000, 0),
        "set initial 20 MHz channel and bandwidth",
        SRC_USB_HALINIT,
    );
    write_reg(
        transfers,
        InitPhase::Channel,
        reg("captured Linux TX bring-up tail", 0x0c20, 0),
        "apply captured runtime RFE, IQK, and TX power register values for bench TX",
        SRC_PHYCFG,
    );
}

fn add_llt_write(transfers: &mut Vec<PlannedInitTransfer>, address: u8, data: u8) {
    let _encoded_operation = (u32::from(address) << 8) | u32::from(data) | (1u32 << 30);
    write_reg(
        transfers,
        InitPhase::Llt,
        reg("REG_LLT_INIT", REG_LLT_INIT, 4),
        "write one LLT entry",
        SRC_HAL_INIT,
    );
    poll_reg(
        transfers,
        InitPhase::Llt,
        reg("REG_LLT_INIT", REG_LLT_INIT, 4),
        "poll LLT write operation completion",
        SRC_HAL_INIT,
    );
}

fn add_firmware_page_blocks(transfers: &mut Vec<PlannedInitTransfer>, page_len: usize) {
    let mut offset = 0usize;
    let block_count = page_len / MAX_REG_BLOCK_SIZE;
    for _ in 0..block_count {
        firmware_block(
            transfers,
            FW_START_ADDRESS + offset as u16,
            MAX_REG_BLOCK_SIZE,
        );
        offset += MAX_REG_BLOCK_SIZE;
    }

    let remain = page_len - offset;
    let eight_byte_blocks = remain / FIRMWARE_REMAINDER_BLOCK_SIZE;
    for _ in 0..eight_byte_blocks {
        firmware_block(
            transfers,
            FW_START_ADDRESS + offset as u16,
            FIRMWARE_REMAINDER_BLOCK_SIZE,
        );
        offset += FIRMWARE_REMAINDER_BLOCK_SIZE;
    }

    for _ in 0..(page_len - offset) {
        firmware_block(transfers, FW_START_ADDRESS + offset as u16, 1);
        offset += 1;
    }
}

fn firmware_page_description(page: usize) -> &'static str {
    match page {
        0 => "select firmware page 0 in REG_MCUFWDL + 2",
        1 => "select firmware page 1 in REG_MCUFWDL + 2",
        2 => "select firmware page 2 in REG_MCUFWDL + 2",
        3 => "select firmware page 3 in REG_MCUFWDL + 2",
        4 => "select firmware page 4 in REG_MCUFWDL + 2",
        5 => "select firmware page 5 in REG_MCUFWDL + 2",
        6 => "select firmware page 6 in REG_MCUFWDL + 2",
        7 => "select firmware page 7 in REG_MCUFWDL + 2",
        _ => "select firmware page in REG_MCUFWDL + 2",
    }
}

fn reg(name: &'static str, address: u16, width: usize) -> InitRegister {
    InitRegister {
        name,
        address,
        width,
    }
}

fn read_reg(
    transfers: &mut Vec<PlannedInitTransfer>,
    phase: InitPhase,
    register: InitRegister,
    description: &'static str,
    source: &'static str,
) {
    push_transfer(
        transfers,
        phase,
        InitOperation::Read,
        Some(register),
        description,
        source,
        control_read_event(register.address, register.width),
    );
}

fn write_reg(
    transfers: &mut Vec<PlannedInitTransfer>,
    phase: InitPhase,
    register: InitRegister,
    description: &'static str,
    source: &'static str,
) {
    push_transfer(
        transfers,
        phase,
        InitOperation::Write,
        Some(register),
        description,
        source,
        control_write_event(register.address, register.width),
    );
}

fn poll_reg(
    transfers: &mut Vec<PlannedInitTransfer>,
    phase: InitPhase,
    register: InitRegister,
    description: &'static str,
    source: &'static str,
) {
    push_transfer(
        transfers,
        phase,
        InitOperation::Poll,
        Some(register),
        description,
        source,
        control_read_event(register.address, register.width),
    );
}

fn rmw_reg8(
    transfers: &mut Vec<PlannedInitTransfer>,
    phase: InitPhase,
    register: InitRegister,
    description: &'static str,
    source: &'static str,
) {
    read_reg(transfers, phase, register, description, source);
    write_reg(transfers, phase, register, description, source);
}

fn firmware_block(transfers: &mut Vec<PlannedInitTransfer>, address: u16, length: usize) {
    push_transfer(
        transfers,
        InitPhase::FirmwareDownload,
        InitOperation::FirmwareBlock,
        Some(reg("FW_START_ADDRESS + offset", address, length)),
        "write firmware bytes through USB register block write",
        SRC_HAL_INIT,
        control_write_event(address, length),
    );
}

fn push_transfer(
    transfers: &mut Vec<PlannedInitTransfer>,
    phase: InitPhase,
    operation: InitOperation,
    register: Option<InitRegister>,
    description: &'static str,
    source: &'static str,
    event: UsbTraceEvent,
) {
    transfers.push(PlannedInitTransfer {
        phase,
        operation,
        register,
        description,
        source,
        event,
    });
}

fn control_write_event(value: u16, length: usize) -> UsbTraceEvent {
    UsbTraceEvent {
        kind: UsbTraceKind::ControlWrite,
        endpoint: None,
        request_type: Some(0x40),
        request: Some(0x05),
        value: Some(value),
        index: Some(0),
        length: Some(length),
        data_hex: None,
    }
}

fn control_read_event(value: u16, length: usize) -> UsbTraceEvent {
    UsbTraceEvent {
        kind: UsbTraceKind::ControlRead,
        endpoint: None,
        request_type: Some(0xc0),
        request: Some(0x05),
        value: Some(value),
        index: Some(0),
        length: Some(length),
        data_hex: None,
    }
}

#[allow(dead_code)]
fn _documented_bit_values() -> (u16, u8, u8, u32, u32, u8) {
    (
        HCI_TXDMA_EN
            | HCI_RXDMA_EN
            | TXDMA_EN
            | RXDMA_EN
            | PROTOCOL_EN
            | SCHEDULE_EN
            | ENSEC
            | CALTMR_EN,
        MACTXEN | MACRXEN,
        MCUFWDL_EN | FWDL_CHKSUM_RPT | RAM_DL_SEL,
        MCUFWDL_RDY,
        WINTINI_RDY,
        DRVINFO_SZ as u8,
    )
}

#[allow(dead_code)]
fn _documented_init_boundaries() -> (u8, u8, u16) {
    (
        TX_PAGE_BOUNDARY_8812,
        LAST_ENTRY_OF_TX_PKT_BUFFER_8812,
        RX_DMA_BOUNDARY_8812,
    )
}

#[allow(dead_code)]
fn _documented_sources() -> (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
) {
    (
        SRC_USB_HALINIT,
        SRC_HAL_INIT,
        SRC_PWRSEQ,
        SRC_REGS,
        SRC_PHYCFG,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FirmwareSource;

    #[test]
    fn init_plan_includes_source_derived_phase_skeleton() {
        let firmware = FirmwareImage::from_bytes(
            FirmwareSource::InMemory,
            vec![0xaa; INIT_DRY_RUN_FIRMWARE_CHUNK_SIZE + 1],
        )
        .expect("firmware");

        let plan = plan_rtl8812au_init(&firmware).expect("plan");

        assert_eq!(plan.firmware_len, INIT_DRY_RUN_FIRMWARE_CHUNK_SIZE + 1);
        assert_eq!(plan.source_commit, INIT_PLAN_REFERENCE_COMMIT);
        assert_eq!(plan.trace_events().len(), plan.transfers.len());
        assert!(plan
            .phase_counts()
            .iter()
            .any(|count| count.phase == InitPhase::Llt && count.transfers == 512));
        assert!(plan
            .phase_counts()
            .iter()
            .any(|count| count.phase == InitPhase::FirmwareChecksumPoll && count.transfers == 5));
        assert!(plan.transfers.iter().any(|transfer| {
            transfer.phase == InitPhase::FirmwareDownload
                && transfer.operation == InitOperation::FirmwareBlock
                && transfer
                    .register
                    .is_some_and(|register| register.address == FW_START_ADDRESS)
        }));
        assert!(plan
            .transfers
            .iter()
            .any(|transfer| transfer.source == SRC_PWRSEQ));
    }

    #[test]
    fn init_plan_uses_realtek_download_payload_len() {
        let mut bytes = vec![0u8; crate::REALTEK_FIRMWARE_HEADER_LEN];
        bytes[0] = 0x01;
        bytes[1] = 0x95;
        bytes.extend_from_slice(&[0xaa; INIT_DRY_RUN_FIRMWARE_CHUNK_SIZE + 1]);
        let firmware =
            FirmwareImage::from_bytes(FirmwareSource::InMemory, bytes).expect("firmware");

        let plan = plan_rtl8812au_init(&firmware).expect("plan");

        assert_eq!(plan.firmware_len, INIT_DRY_RUN_FIRMWARE_CHUNK_SIZE + 1);
    }
}
