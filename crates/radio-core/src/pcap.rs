use std::{
    io::{self, Write},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::RxFrame;

const PCAP_MAGIC_LE: u32 = 0xa1b2_c3d4;
const PCAP_VERSION_MAJOR: u16 = 2;
const PCAP_VERSION_MINOR: u16 = 4;
const PCAP_SNAPLEN: u32 = 65_535;
const LINKTYPE_IEEE802_11: u32 = 105;
const USEC_PER_SEC: u128 = 1_000_000;

pub struct PcapWriter<W> {
    writer: W,
}

impl<W: Write> PcapWriter<W> {
    pub fn new(mut writer: W) -> io::Result<Self> {
        writer.write_all(&PCAP_MAGIC_LE.to_le_bytes())?;
        writer.write_all(&PCAP_VERSION_MAJOR.to_le_bytes())?;
        writer.write_all(&PCAP_VERSION_MINOR.to_le_bytes())?;
        writer.write_all(&0i32.to_le_bytes())?;
        writer.write_all(&0u32.to_le_bytes())?;
        writer.write_all(&PCAP_SNAPLEN.to_le_bytes())?;
        writer.write_all(&LINKTYPE_IEEE802_11.to_le_bytes())?;
        Ok(Self { writer })
    }

    pub fn write_frame(&mut self, timestamp: SystemTime, frame: &[u8]) -> io::Result<()> {
        let elapsed = timestamp
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros();
        let ts_sec = (elapsed / USEC_PER_SEC).min(u32::MAX as u128) as u32;
        let ts_usec = (elapsed % USEC_PER_SEC) as u32;
        let captured_len = frame.len().min(PCAP_SNAPLEN as usize) as u32;

        self.writer.write_all(&ts_sec.to_le_bytes())?;
        self.writer.write_all(&ts_usec.to_le_bytes())?;
        self.writer.write_all(&captured_len.to_le_bytes())?;
        self.writer.write_all(&(frame.len() as u32).to_le_bytes())?;
        self.writer.write_all(&frame[..captured_len as usize])?;
        Ok(())
    }

    pub fn write_rx_frame(&mut self, timestamp: SystemTime, frame: &RxFrame) -> io::Result<()> {
        self.write_frame(timestamp, &frame.data)
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }

    pub fn into_inner(self) -> W {
        self.writer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_pcap_header_and_raw_80211_packet() {
        let mut writer = PcapWriter::new(Vec::new()).expect("pcap");
        writer
            .write_frame(
                UNIX_EPOCH + std::time::Duration::from_micros(1_234_567),
                &[1, 2, 3],
            )
            .expect("packet");

        let bytes = writer.into_inner();

        assert_eq!(&bytes[0..4], &PCAP_MAGIC_LE.to_le_bytes());
        assert_eq!(&bytes[20..24], &LINKTYPE_IEEE802_11.to_le_bytes());
        assert_eq!(&bytes[24..28], &1u32.to_le_bytes());
        assert_eq!(&bytes[28..32], &234_567u32.to_le_bytes());
        assert_eq!(&bytes[32..36], &3u32.to_le_bytes());
        assert_eq!(&bytes[36..40], &3u32.to_le_bytes());
        assert_eq!(&bytes[40..43], &[1, 2, 3]);
    }
}
