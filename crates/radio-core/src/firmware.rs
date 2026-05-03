use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::realtek_table::{parse_realtek_u8_array, RealtekTableError};
use serde::Serialize;
use thiserror::Error;

pub const RTL8812A_FIRMWARE_MAX_LEN: usize = 128 * 1024;
pub const REALTEK_FIRMWARE_HEADER_LEN: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FirmwareImage {
    pub source: FirmwareSource,
    pub len: usize,
    pub byte_sum: u32,
    bytes: Vec<u8>,
}

impl FirmwareImage {
    pub fn from_bytes(source: FirmwareSource, bytes: Vec<u8>) -> Result<Self, FirmwareError> {
        if bytes.is_empty() {
            return Err(FirmwareError::Empty);
        }
        if bytes.len() > RTL8812A_FIRMWARE_MAX_LEN {
            return Err(FirmwareError::TooLarge {
                max_len: RTL8812A_FIRMWARE_MAX_LEN,
                actual_len: bytes.len(),
            });
        }

        let byte_sum = bytes
            .iter()
            .fold(0u32, |acc, byte| acc.wrapping_add(u32::from(*byte)));
        Ok(Self {
            source,
            len: bytes.len(),
            byte_sum,
            bytes,
        })
    }

    pub fn load_external(path: impl AsRef<Path>) -> Result<Self, FirmwareError> {
        let path = path.as_ref();
        let bytes = fs::read(path).map_err(|source| FirmwareError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_bytes(FirmwareSource::External(path.to_path_buf()), bytes)
    }

    pub fn load_realtek_c_array(
        path: impl AsRef<Path>,
        array_name: &str,
    ) -> Result<Self, FirmwareError> {
        let path = path.as_ref();
        let source = fs::read_to_string(path).map_err(|source| FirmwareError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_realtek_c_array_source(
            FirmwareSource::RealtekCArray {
                path: path.to_path_buf(),
                array_name: array_name.to_string(),
            },
            &source,
            array_name,
        )
    }

    pub fn from_realtek_c_array_source(
        source: FirmwareSource,
        c_source: &str,
        array_name: &str,
    ) -> Result<Self, FirmwareError> {
        let bytes = parse_realtek_u8_array(c_source, array_name).map_err(|source| {
            FirmwareError::RealtekArrayParse {
                array_name: array_name.to_string(),
                source,
            }
        })?;
        Self::from_bytes(source, bytes)
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn chunks(&self, chunk_size: usize) -> Result<FirmwareChunks<'_>, FirmwareError> {
        if chunk_size == 0 {
            return Err(FirmwareError::InvalidChunkSize);
        }
        Ok(FirmwareChunks {
            bytes: &self.bytes,
            chunk_size,
            offset: 0,
        })
    }

    pub fn realtek_download_payload(&self) -> FirmwarePayload<'_> {
        let signature = self
            .bytes
            .get(..2)
            .map(|signature| u16::from_le_bytes([signature[0], signature[1]]));
        let has_header = signature
            .map(|signature| {
                let signature_family = signature & 0xfff0;
                signature_family == 0x9500 || signature_family == 0x2100
            })
            .unwrap_or(false);
        let offset = if has_header && self.bytes.len() > REALTEK_FIRMWARE_HEADER_LEN {
            REALTEK_FIRMWARE_HEADER_LEN
        } else {
            0
        };

        FirmwarePayload {
            bytes: &self.bytes[offset..],
            offset,
            signature,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FirmwareSource {
    External(PathBuf),
    RealtekCArray { path: PathBuf, array_name: String },
    Embedded(&'static str),
    InMemory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareChunk<'a> {
    pub offset: usize,
    pub bytes: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwarePayload<'a> {
    pub bytes: &'a [u8],
    pub offset: usize,
    pub signature: Option<u16>,
}

pub struct FirmwareChunks<'a> {
    bytes: &'a [u8],
    chunk_size: usize,
    offset: usize,
}

impl<'a> Iterator for FirmwareChunks<'a> {
    type Item = FirmwareChunk<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.bytes.len() {
            return None;
        }

        let offset = self.offset;
        let end = (offset + self.chunk_size).min(self.bytes.len());
        self.offset = end;
        Some(FirmwareChunk {
            offset,
            bytes: &self.bytes[offset..end],
        })
    }
}

#[derive(Debug, Error)]
pub enum FirmwareError {
    #[error("firmware image is empty")]
    Empty,
    #[error("firmware image too large: max {max_len} bytes, got {actual_len}")]
    TooLarge { max_len: usize, actual_len: usize },
    #[error("firmware chunk size must be greater than zero")]
    InvalidChunkSize,
    #[error("failed to read firmware image {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse firmware C array {array_name:?}: {source}")]
    RealtekArrayParse {
        array_name: String,
        source: RealtekTableError,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_summarizes_firmware_bytes() {
        let firmware =
            FirmwareImage::from_bytes(FirmwareSource::InMemory, vec![1, 2, 3]).expect("firmware");

        assert_eq!(firmware.len, 3);
        assert_eq!(firmware.byte_sum, 6);
        assert_eq!(firmware.bytes(), &[1, 2, 3]);
    }

    #[test]
    fn rejects_empty_firmware() {
        assert!(matches!(
            FirmwareImage::from_bytes(FirmwareSource::InMemory, Vec::new()),
            Err(FirmwareError::Empty)
        ));
    }

    #[test]
    fn iterates_firmware_chunks_with_offsets() {
        let firmware = FirmwareImage::from_bytes(FirmwareSource::InMemory, vec![1, 2, 3, 4, 5])
            .expect("firmware");

        let chunks: Vec<_> = firmware.chunks(2).expect("chunks").collect();

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].offset, 0);
        assert_eq!(chunks[0].bytes, &[1, 2]);
        assert_eq!(chunks[2].offset, 4);
        assert_eq!(chunks[2].bytes, &[5]);
    }

    #[test]
    fn skips_realtek_firmware_header_for_download_payload() {
        let mut bytes = vec![0u8; REALTEK_FIRMWARE_HEADER_LEN];
        bytes[0] = 0x01;
        bytes[1] = 0x95;
        bytes.extend_from_slice(&[0xaa, 0xbb, 0xcc]);
        let firmware =
            FirmwareImage::from_bytes(FirmwareSource::InMemory, bytes).expect("firmware");

        let payload = firmware.realtek_download_payload();

        assert_eq!(payload.offset, REALTEK_FIRMWARE_HEADER_LEN);
        assert_eq!(payload.signature, Some(0x9501));
        assert_eq!(payload.bytes, &[0xaa, 0xbb, 0xcc]);
    }

    #[test]
    fn loads_firmware_from_realtek_c_array_source() {
        let source = r#"
            u8 array_mp_8812a_fw_nic[] = {
                0x01, 0x95, 0x00, 0x00,
            };
        "#;
        let firmware = FirmwareImage::from_realtek_c_array_source(
            FirmwareSource::InMemory,
            source,
            "array_mp_8812a_fw_nic",
        )
        .expect("firmware");

        assert_eq!(firmware.len, 4);
        assert_eq!(firmware.bytes(), &[0x01, 0x95, 0x00, 0x00]);
        assert_eq!(firmware.realtek_download_payload().signature, Some(0x9501));
    }
}
