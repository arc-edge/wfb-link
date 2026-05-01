use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsbTraceEvent {
    pub kind: UsbTraceKind,
    pub endpoint: Option<u8>,
    pub request_type: Option<u8>,
    pub request: Option<u8>,
    pub value: Option<u16>,
    pub index: Option<u16>,
    pub length: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_hex: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UsbTraceImport {
    pub events: Vec<UsbTraceEvent>,
    pub ignored_lines: usize,
    pub errors: Vec<UsbTraceImportError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UsbTraceImportError {
    pub line_number: usize,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsbTraceKind {
    ControlRead,
    ControlWrite,
    BulkIn,
    BulkOut,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UsbTraceComparison {
    pub result: UsbTraceComparisonResult,
    pub expected_len: usize,
    pub observed_len: usize,
    pub compared_len: usize,
    pub mismatches: Vec<UsbTraceMismatch>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UsbTraceComparisonResult {
    Pass,
    Fail,
}

impl UsbTraceComparisonResult {
    pub fn is_failure(self) -> bool {
        matches!(self, Self::Fail)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UsbTraceMismatch {
    pub event_index: usize,
    pub field: &'static str,
    pub expected: String,
    pub observed: String,
}

pub fn compare_usb_traces(
    expected: &[UsbTraceEvent],
    observed: &[UsbTraceEvent],
) -> UsbTraceComparison {
    let compared_len = expected.len().min(observed.len());
    let mut mismatches = Vec::new();

    for idx in 0..compared_len {
        compare_event(idx, &expected[idx], &observed[idx], &mut mismatches);
    }

    if expected.len() != observed.len() {
        mismatches.push(UsbTraceMismatch {
            event_index: compared_len,
            field: "event_count",
            expected: expected.len().to_string(),
            observed: observed.len().to_string(),
        });
    }

    UsbTraceComparison {
        result: if mismatches.is_empty() {
            UsbTraceComparisonResult::Pass
        } else {
            UsbTraceComparisonResult::Fail
        },
        expected_len: expected.len(),
        observed_len: observed.len(),
        compared_len,
        mismatches,
    }
}

pub fn import_usbmon_text(input: &str) -> UsbTraceImport {
    let mut events = Vec::new();
    let mut ignored_lines = 0usize;
    let mut errors = Vec::new();

    for (idx, line) in input.lines().enumerate() {
        let line_number = idx + 1;
        match parse_usbmon_line(line) {
            UsbmonLineParse::Event(event) => events.push(event),
            UsbmonLineParse::Ignored => ignored_lines += 1,
            UsbmonLineParse::Error(message) => errors.push(UsbTraceImportError {
                line_number,
                message,
            }),
        }
    }

    UsbTraceImport {
        events,
        ignored_lines,
        errors,
    }
}

enum UsbmonLineParse {
    Event(UsbTraceEvent),
    Ignored,
    Error(String),
}

fn parse_usbmon_line(line: &str) -> UsbmonLineParse {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return UsbmonLineParse::Ignored;
    }

    let fields: Vec<_> = trimmed.split_whitespace().collect();
    if fields.len() < 4 {
        return UsbmonLineParse::Ignored;
    }
    if fields[2] != "S" {
        return UsbmonLineParse::Ignored;
    }

    let address = fields[3];
    if let Some(rest) = address.strip_prefix("C") {
        parse_control_submit(rest, &fields)
    } else if let Some(rest) = address.strip_prefix("B") {
        parse_bulk_submit(rest, &fields)
    } else {
        UsbmonLineParse::Ignored
    }
}

fn parse_control_submit(direction_and_address: &str, fields: &[&str]) -> UsbmonLineParse {
    if fields.len() < 10 {
        return UsbmonLineParse::Error("control submit line is too short".to_string());
    }
    if fields[4] != "s" {
        return UsbmonLineParse::Ignored;
    }

    let kind = match direction_and_address.chars().next() {
        Some('i') => UsbTraceKind::ControlRead,
        Some('o') => UsbTraceKind::ControlWrite,
        _ => return UsbmonLineParse::Ignored,
    };

    let request_type = match parse_hex_u8(fields[5]) {
        Ok(value) => value,
        Err(message) => return UsbmonLineParse::Error(format!("request_type: {message}")),
    };
    let request = match parse_hex_u8(fields[6]) {
        Ok(value) => value,
        Err(message) => return UsbmonLineParse::Error(format!("request: {message}")),
    };
    let value = match parse_hex_u16(fields[7]) {
        Ok(value) => value,
        Err(message) => return UsbmonLineParse::Error(format!("value: {message}")),
    };
    let index = match parse_hex_u16(fields[8]) {
        Ok(value) => value,
        Err(message) => return UsbmonLineParse::Error(format!("index: {message}")),
    };
    let length = match parse_hex_usize(fields[9]) {
        Ok(value) => value,
        Err(message) => return UsbmonLineParse::Error(format!("length: {message}")),
    };

    UsbmonLineParse::Event(UsbTraceEvent {
        kind,
        endpoint: None,
        request_type: Some(request_type),
        request: Some(request),
        value: Some(value),
        index: Some(index),
        length: Some(length),
        data_hex: parse_usbmon_data_hex(fields),
    })
}

fn parse_bulk_submit(direction_and_address: &str, fields: &[&str]) -> UsbmonLineParse {
    let kind = match direction_and_address.chars().next() {
        Some('i') => UsbTraceKind::BulkIn,
        Some('o') => UsbTraceKind::BulkOut,
        _ => return UsbmonLineParse::Ignored,
    };
    let endpoint_number = match parse_usbmon_endpoint(fields[3]) {
        Ok(endpoint) => endpoint,
        Err(message) => return UsbmonLineParse::Error(message),
    };
    let endpoint = if matches!(kind, UsbTraceKind::BulkIn) {
        0x80 | endpoint_number
    } else {
        endpoint_number
    };
    let length = match find_usbmon_bulk_length(fields) {
        Some(length) => length,
        None => return UsbmonLineParse::Error("bulk submit line has no length field".to_string()),
    };

    UsbmonLineParse::Event(UsbTraceEvent {
        kind,
        endpoint: Some(endpoint),
        request_type: None,
        request: None,
        value: None,
        index: None,
        length: Some(length),
        data_hex: parse_usbmon_data_hex(fields),
    })
}

fn parse_usbmon_endpoint(address: &str) -> Result<u8, String> {
    let endpoint = address
        .rsplit(':')
        .next()
        .ok_or_else(|| format!("invalid usbmon address {address:?}"))?;
    endpoint
        .parse::<u8>()
        .map_err(|error| format!("invalid endpoint in {address:?}: {error}"))
}

fn find_usbmon_bulk_length(fields: &[&str]) -> Option<usize> {
    fields
        .iter()
        .skip(5)
        .find_map(|field| field.parse::<isize>().ok().filter(|value| *value >= 0))
        .map(|value| value as usize)
}

fn parse_usbmon_data_hex(fields: &[&str]) -> Option<String> {
    let equals_idx = fields.iter().position(|field| *field == "=")?;
    let mut data = String::new();
    for field in fields.iter().skip(equals_idx + 1) {
        if !field.chars().all(|ch| ch.is_ascii_hexdigit()) {
            break;
        }
        data.push_str(field);
    }
    if data.is_empty() {
        None
    } else {
        Some(data.to_ascii_lowercase())
    }
}

fn parse_hex_u8(input: &str) -> Result<u8, String> {
    u8::from_str_radix(strip_hex_prefix(input), 16).map_err(|error| error.to_string())
}

fn parse_hex_u16(input: &str) -> Result<u16, String> {
    u16::from_str_radix(strip_hex_prefix(input), 16).map_err(|error| error.to_string())
}

fn parse_hex_usize(input: &str) -> Result<usize, String> {
    usize::from_str_radix(strip_hex_prefix(input), 16).map_err(|error| error.to_string())
}

fn strip_hex_prefix(input: &str) -> &str {
    input
        .strip_prefix("0x")
        .or_else(|| input.strip_prefix("0X"))
        .unwrap_or(input)
}

fn compare_event(
    event_index: usize,
    expected: &UsbTraceEvent,
    observed: &UsbTraceEvent,
    mismatches: &mut Vec<UsbTraceMismatch>,
) {
    push_mismatch(
        event_index,
        "kind",
        &expected.kind,
        &observed.kind,
        mismatches,
    );
    push_mismatch(
        event_index,
        "endpoint",
        &expected.endpoint,
        &observed.endpoint,
        mismatches,
    );
    push_mismatch(
        event_index,
        "request_type",
        &expected.request_type,
        &observed.request_type,
        mismatches,
    );
    push_mismatch(
        event_index,
        "request",
        &expected.request,
        &observed.request,
        mismatches,
    );
    push_mismatch(
        event_index,
        "value",
        &expected.value,
        &observed.value,
        mismatches,
    );
    push_mismatch(
        event_index,
        "index",
        &expected.index,
        &observed.index,
        mismatches,
    );
    push_mismatch(
        event_index,
        "length",
        &expected.length,
        &observed.length,
        mismatches,
    );
    if expected.data_hex.is_some() {
        push_mismatch(
            event_index,
            "data_hex",
            &expected.data_hex,
            &observed.data_hex,
            mismatches,
        );
    }
}

fn push_mismatch<T: PartialEq + std::fmt::Debug>(
    event_index: usize,
    field: &'static str,
    expected: &T,
    observed: &T,
    mismatches: &mut Vec<UsbTraceMismatch>,
) {
    if expected != observed {
        mismatches.push(UsbTraceMismatch {
            event_index,
            field,
            expected: format!("{expected:?}"),
            observed: format!("{observed:?}"),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(value: u16) -> UsbTraceEvent {
        UsbTraceEvent {
            kind: UsbTraceKind::ControlWrite,
            endpoint: None,
            request_type: Some(0x40),
            request: Some(0x05),
            value: Some(value),
            index: Some(0),
            length: Some(1),
            data_hex: None,
        }
    }

    #[test]
    fn matching_trace_sequences_pass() {
        let expected = [event(0x0001)];
        let observed = [event(0x0001)];

        let comparison = compare_usb_traces(&expected, &observed);

        assert_eq!(comparison.result, UsbTraceComparisonResult::Pass);
        assert!(comparison.mismatches.is_empty());
    }

    #[test]
    fn mismatched_trace_sequences_report_field() {
        let expected = [event(0x0001)];
        let observed = [event(0x0002)];

        let comparison = compare_usb_traces(&expected, &observed);

        assert_eq!(comparison.result, UsbTraceComparisonResult::Fail);
        assert_eq!(comparison.mismatches[0].field, "value");
    }

    #[test]
    fn trace_comparison_treats_missing_expected_payload_as_unknown() {
        let expected = [event(0x0001)];
        let mut observed_event = event(0x0001);
        observed_event.data_hex = Some("00".to_string());
        let observed = [observed_event];

        let comparison = compare_usb_traces(&expected, &observed);

        assert_eq!(comparison.result, UsbTraceComparisonResult::Pass);
        assert!(comparison.mismatches.is_empty());
    }

    #[test]
    fn trace_comparison_reports_known_payload_mismatch() {
        let mut expected_event = event(0x0001);
        expected_event.data_hex = Some("00".to_string());
        let mut observed_event = event(0x0001);
        observed_event.data_hex = Some("01".to_string());

        let comparison = compare_usb_traces(&[expected_event], &[observed_event]);

        assert_eq!(comparison.result, UsbTraceComparisonResult::Fail);
        assert_eq!(comparison.mismatches[0].field, "data_hex");
    }

    #[test]
    fn imports_usbmon_control_submit_lines() {
        let imported = import_usbmon_text(
            "\
ffff 0 S Co:1:004:0 s 40 05 0002 0000 0001 1 = 00
ffff 1 S Ci:1:004:0 s c0 05 0002 0000 0004 4 <
",
        );

        assert!(imported.errors.is_empty());
        assert_eq!(imported.events.len(), 2);
        assert_eq!(imported.events[0].kind, UsbTraceKind::ControlWrite);
        assert_eq!(imported.events[0].request_type, Some(0x40));
        assert_eq!(imported.events[0].request, Some(0x05));
        assert_eq!(imported.events[0].value, Some(0x0002));
        assert_eq!(imported.events[0].length, Some(1));
        assert_eq!(imported.events[0].data_hex.as_deref(), Some("00"));
        assert_eq!(imported.events[1].kind, UsbTraceKind::ControlRead);
        assert_eq!(imported.events[1].request_type, Some(0xc0));
        assert_eq!(imported.events[1].length, Some(4));
        assert_eq!(imported.events[1].data_hex, None);
    }

    #[test]
    fn imports_usbmon_bulk_submit_lines() {
        let imported = import_usbmon_text(
            "\
ffff 0 S Bo:1:004:2 -115 64 = 01020304
ffff 1 S Bi:1:004:1 -115 512 <
ffff 2 C Bi:1:004:1 0 64 = 01020304
",
        );

        assert!(imported.errors.is_empty());
        assert_eq!(imported.events.len(), 2);
        assert_eq!(imported.events[0].kind, UsbTraceKind::BulkOut);
        assert_eq!(imported.events[0].endpoint, Some(0x02));
        assert_eq!(imported.events[0].length, Some(64));
        assert_eq!(imported.events[0].data_hex.as_deref(), Some("01020304"));
        assert_eq!(imported.events[1].kind, UsbTraceKind::BulkIn);
        assert_eq!(imported.events[1].endpoint, Some(0x81));
        assert_eq!(imported.events[1].length, Some(512));
        assert_eq!(imported.events[1].data_hex, None);
        assert_eq!(imported.ignored_lines, 1);
    }

    #[test]
    fn imports_spaced_usbmon_payload_tokens() {
        let imported = import_usbmon_text(
            "\
ffff 0 S Co:1:004:0 s 40 05 0cb0 0000 0004 4 = 17773354
ffff 1 S Bo:1:004:2 -115 91 = 3300288d 01120800 00000000
",
        );

        assert!(imported.errors.is_empty());
        assert_eq!(imported.events.len(), 2);
        assert_eq!(imported.events[0].data_hex.as_deref(), Some("17773354"));
        assert_eq!(
            imported.events[1].data_hex.as_deref(),
            Some("3300288d0112080000000000")
        );
    }
}
