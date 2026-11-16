//! Bounded UTF-8 CSV for a deliberately narrow, explicit dialect.
//!
//! This is not a general RFC 4180 claim. The delimiter is one printable ASCII
//! byte other than quote. Quotes use double-quote escaping. Fields are quoted
//! when empty or containing delimiter, quote, CR, or LF. Outside quoted fields,
//! records are separated by CRLF. Inside quoted fields, CR/LF bytes are field
//! data and are preserved verbatim; bare CR/LF elsewhere is rejected. Encoded records
//! have no trailing CRLF. There is no header interpretation.

use std::fmt;

pub const CONTRACT: &str = "goya.data.csv.bounded_crlf.v1";
pub const RECIPE: &str = "rust-std-bounded-crlf-csv";
pub const RECIPE_VERSION: &str = "0.1.0";
pub const MAX_TOTAL_BYTES: u32 = 16 * 1024 * 1024;
pub const MAX_RECORDS: u32 = 100_000;
pub const MAX_CELLS: u32 = 1_000_000;
pub const MAX_FIELDS_PER_RECORD: u32 = 4096;
pub const MAX_FIELD_BYTES: u32 = 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Declaration {
    pub delimiter: u8,
    pub max_total_bytes: u32,
    pub max_records: u32,
    pub max_cells: u32,
    pub max_fields_per_record: u32,
    pub max_field_bytes: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    ProcessLocalRust,
    Browser,
    Node,
    CrossProcess,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostProfile {
    pub pointer_width_bits: u8,
    pub standard_library: bool,
    pub utf8_strings: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    InvalidDelimiter,
    ZeroLimit,
    LimitTooLarge,
    UnsupportedTarget,
    UnsupportedPointerWidth,
    AllocationLimitExceeded,
    MissingStandardLibrary,
    MissingUtf8Strings,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidDelimiter => "delimiter must be printable ASCII other than quote",
            Self::ZeroLimit => "CSV limits must be positive",
            Self::LimitTooLarge => "CSV limits exceed the supported recipe bounds",
            Self::UnsupportedTarget => "CSV recipe supports only process-local Rust",
            Self::UnsupportedPointerWidth => "target pointer width must be 32 or 64 bits",
            Self::AllocationLimitExceeded => "CSV byte bound exceeds target Vec allocation bounds",
            Self::MissingStandardLibrary => "target requires the Rust standard library",
            Self::MissingUtf8Strings => "target requires UTF-8 string support",
        })
    }
}

impl std::error::Error for Diagnostic {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CsvError {
    InvalidLimits,
    InvalidDelimiter,
    EmptyRecord,
    TooManyRecords { maximum: usize },
    TooManyCells { maximum: usize },
    TooManyFields { maximum: usize },
    FieldTooLarge { maximum: usize },
    InputTooLarge { maximum: usize },
    EncodedTooLarge { maximum: usize },
    AllocationFailed { requested: usize },
    MalformedQuote,
    UnterminatedQuotedField,
    UnexpectedAfterQuote,
    InvalidRecordSeparator,
    TrailingRecordSeparator,
    InvalidUtf8,
}

impl fmt::Display for CsvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits => f.write_str("CSV limits are invalid"),
            Self::InvalidDelimiter => f.write_str("CSV delimiter is not supported"),
            Self::EmptyRecord => f.write_str("records must contain at least one field"),
            Self::TooManyRecords { maximum } => write!(f, "record count exceeds {maximum}"),
            Self::TooManyCells { maximum } => write!(f, "cell count exceeds {maximum}"),
            Self::TooManyFields { maximum } => write!(f, "field count exceeds {maximum}"),
            Self::FieldTooLarge { maximum } => write!(f, "field exceeds {maximum} UTF-8 bytes"),
            Self::InputTooLarge { maximum } => write!(f, "CSV input exceeds {maximum} bytes"),
            Self::EncodedTooLarge { maximum } => write!(f, "encoded CSV exceeds {maximum} bytes"),
            Self::AllocationFailed { requested } => {
                write!(f, "unable to reserve {requested} CSV bytes")
            }
            Self::MalformedQuote => f.write_str("quote appears inside an unquoted field"),
            Self::UnterminatedQuotedField => f.write_str("quoted field is not terminated"),
            Self::UnexpectedAfterQuote => f.write_str("unexpected byte after closing quote"),
            Self::InvalidRecordSeparator => f.write_str("records must be separated by CRLF"),
            Self::TrailingRecordSeparator => {
                f.write_str("CSV must not end with a record separator")
            }
            Self::InvalidUtf8 => f.write_str("CSV field is not valid UTF-8"),
        }
    }
}

impl std::error::Error for CsvError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CsvLimits {
    pub delimiter: u8,
    pub max_total_bytes: usize,
    pub max_records: usize,
    pub max_cells: usize,
    pub max_fields_per_record: usize,
    pub max_field_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ir(CsvLimits);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Artifact {
    pub contract: &'static str,
    pub recipe: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub target_pointer_width_bits: u8,
    pub source: String,
}

impl Declaration {
    pub fn normalize(self) -> Result<Ir, Diagnostic> {
        if !valid_delimiter(self.delimiter) {
            return Err(Diagnostic::InvalidDelimiter);
        }
        if self.max_total_bytes == 0
            || self.max_records == 0
            || self.max_cells == 0
            || self.max_fields_per_record == 0
            || self.max_field_bytes == 0
        {
            return Err(Diagnostic::ZeroLimit);
        }
        if self.max_total_bytes > MAX_TOTAL_BYTES
            || self.max_records > MAX_RECORDS
            || self.max_cells > MAX_CELLS
            || self.max_fields_per_record > MAX_FIELDS_PER_RECORD
            || self.max_field_bytes > MAX_FIELD_BYTES
        {
            return Err(Diagnostic::LimitTooLarge);
        }
        Ok(Ir(CsvLimits {
            delimiter: self.delimiter,
            max_total_bytes: self.max_total_bytes as usize,
            max_records: self.max_records as usize,
            max_cells: self.max_cells as usize,
            max_fields_per_record: self.max_fields_per_record as usize,
            max_field_bytes: self.max_field_bytes as usize,
        }))
    }
}

impl Ir {
    pub fn canonical(&self) -> String {
        let limits = self.0;
        format!(
            "contract={CONTRACT}\nrecipe={RECIPE}@{RECIPE_VERSION}\ndialect=bounded-utf8-crlf-v1\ndelimiter_byte={}\nquote=double-quote\nescape=double-quote-doubling\nquote_fields=empty_or_containing_delimiter_quote_cr_lf\nrecord_separator=CRLF\nquoted_cr_lf=preserved_verbatim\ntrailing_record_separator=forbidden\nheader=ordinary_record\nmax_total_bytes={}\nmax_records={}\nmax_cells={}\nmax_fields_per_record={}\nmax_field_bytes={}\n",
            limits.delimiter,
            limits.max_total_bytes,
            limits.max_records,
            limits.max_cells,
            limits.max_fields_per_record,
            limits.max_field_bytes
        )
    }

    pub fn lower_checked(&self, target: Target, host: HostProfile) -> Result<Artifact, Diagnostic> {
        if target != Target::ProcessLocalRust {
            return Err(Diagnostic::UnsupportedTarget);
        }
        let isize_max = match host.pointer_width_bits {
            32 => i32::MAX as u64,
            64 => i64::MAX as u64,
            _ => return Err(Diagnostic::UnsupportedPointerWidth),
        };
        if u64::from(self.0.max_total_bytes as u32) > isize_max {
            return Err(Diagnostic::AllocationLimitExceeded);
        }
        if !host.standard_library {
            return Err(Diagnostic::MissingStandardLibrary);
        }
        if !host.utf8_strings {
            return Err(Diagnostic::MissingUtf8Strings);
        }
        let limits = self.0;
        let canonical_ir = format!(
            "{}target=process_local_rust\ntarget_pointer_width_bits={}\n",
            self.canonical(),
            host.pointer_width_bits
        );
        let source = format!(
            "// {CONTRACT}; recipe={RECIPE}@{RECIPE_VERSION}; bounded UTF-8 CRLF subset, no general CSV compatibility claim.\n\
             pub const CSV_LIMITS: crate::csv_caprichio::CsvLimits = crate::csv_caprichio::CsvLimits {{\n\
             delimiter: {}u8, max_total_bytes: {}usize, max_records: {}usize, max_cells: {}usize,\n\
             max_fields_per_record: {}usize, max_field_bytes: {}usize }};\n\
             pub fn encode_csv(records: &[Vec<String>]) -> Result<String, crate::csv_caprichio::CsvError> {{\n\
                 crate::csv_caprichio::encode(records, CSV_LIMITS)\n\
             }}\n\
             pub fn decode_csv(input: &str) -> Result<Vec<Vec<String>>, crate::csv_caprichio::CsvError> {{\n\
                 crate::csv_caprichio::decode(input, CSV_LIMITS)\n\
             }}\n",
            limits.delimiter,
            limits.max_total_bytes,
            limits.max_records,
            limits.max_cells,
            limits.max_fields_per_record,
            limits.max_field_bytes
        );
        Ok(Artifact {
            contract: CONTRACT,
            recipe: RECIPE,
            recipe_version: RECIPE_VERSION,
            canonical_ir,
            target_pointer_width_bits: host.pointer_width_bits,
            source,
        })
    }
}

/// Encode records using the exact dialect documented in this module. A zero
/// record input encodes as empty text; each record must have at least one field.
pub fn encode(records: &[Vec<String>], limits: CsvLimits) -> Result<String, CsvError> {
    validate_limits(limits)?;
    if records.is_empty() {
        return Ok(String::new());
    }
    if records.len() > limits.max_records {
        return Err(CsvError::TooManyRecords {
            maximum: limits.max_records,
        });
    }
    let mut output = String::new();
    let mut cells = 0usize;
    for (record_index, record) in records.iter().enumerate() {
        if record.is_empty() {
            return Err(CsvError::EmptyRecord);
        }
        if record.len() > limits.max_fields_per_record {
            return Err(CsvError::TooManyFields {
                maximum: limits.max_fields_per_record,
            });
        }
        cells = cells
            .checked_add(record.len())
            .ok_or(CsvError::TooManyCells {
                maximum: limits.max_cells,
            })?;
        if cells > limits.max_cells {
            return Err(CsvError::TooManyCells {
                maximum: limits.max_cells,
            });
        }
        if record_index > 0 {
            append_str(&mut output, "\r\n", limits.max_total_bytes)?;
        }
        for (field_index, field) in record.iter().enumerate() {
            if field.len() > limits.max_field_bytes {
                return Err(CsvError::FieldTooLarge {
                    maximum: limits.max_field_bytes,
                });
            }
            if field_index > 0 {
                append_byte(&mut output, limits.delimiter, limits.max_total_bytes)?;
            }
            let quoted = field.is_empty()
                || field.bytes().any(|byte| {
                    byte == limits.delimiter || byte == b'"' || byte == b'\r' || byte == b'\n'
                });
            if quoted {
                append_byte(&mut output, b'"', limits.max_total_bytes)?;
                for ch in field.chars() {
                    if ch == '"' {
                        append_str(&mut output, "\"\"", limits.max_total_bytes)?;
                    } else {
                        append_char(&mut output, ch, limits.max_total_bytes)?;
                    }
                }
                append_byte(&mut output, b'"', limits.max_total_bytes)?;
            } else {
                append_str(&mut output, field, limits.max_total_bytes)?;
            }
        }
    }
    Ok(output)
}

/// Decode only the declared UTF-8 CRLF dialect. All row, cell and field bounds
/// are enforced before returning values; quoted CRLF bytes are field content.
pub fn decode(input: &str, limits: CsvLimits) -> Result<Vec<Vec<String>>, CsvError> {
    validate_limits(limits)?;
    if input.len() > limits.max_total_bytes {
        return Err(CsvError::InputTooLarge {
            maximum: limits.max_total_bytes,
        });
    }
    if input.is_empty() {
        return Ok(Vec::new());
    }
    let bytes = input.as_bytes();
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field: Vec<u8> = Vec::new();
    let mut cells = 0usize;
    let mut index = 0usize;
    let mut state = ParseState::StartField;

    loop {
        let byte = bytes.get(index).copied();
        match state {
            ParseState::StartField => match byte {
                None => {
                    finish_field(&mut field, &mut record, &mut cells, limits)?;
                    finish_record(&mut record, &mut records, limits)?;
                    break;
                }
                Some(b'"') => {
                    state = ParseState::Quoted;
                    index += 1;
                }
                Some(b) if b == limits.delimiter => {
                    finish_field(&mut field, &mut record, &mut cells, limits)?;
                    index += 1;
                }
                Some(b'\r') if bytes.get(index + 1) == Some(&b'\n') => {
                    finish_field(&mut field, &mut record, &mut cells, limits)?;
                    finish_record(&mut record, &mut records, limits)?;
                    index += 2;
                    if index == bytes.len() {
                        return Err(CsvError::TrailingRecordSeparator);
                    }
                }
                Some(b'\r' | b'\n') => return Err(CsvError::InvalidRecordSeparator),
                Some(_) => state = ParseState::Unquoted,
            },
            ParseState::Unquoted => match byte {
                None => {
                    finish_field(&mut field, &mut record, &mut cells, limits)?;
                    finish_record(&mut record, &mut records, limits)?;
                    break;
                }
                Some(b) if b == limits.delimiter => {
                    finish_field(&mut field, &mut record, &mut cells, limits)?;
                    state = ParseState::StartField;
                    index += 1;
                }
                Some(b'\r') if bytes.get(index + 1) == Some(&b'\n') => {
                    finish_field(&mut field, &mut record, &mut cells, limits)?;
                    finish_record(&mut record, &mut records, limits)?;
                    state = ParseState::StartField;
                    index += 2;
                    if index == bytes.len() {
                        return Err(CsvError::TrailingRecordSeparator);
                    }
                }
                Some(b'\r' | b'\n') => return Err(CsvError::InvalidRecordSeparator),
                Some(b'"') => return Err(CsvError::MalformedQuote),
                Some(b) => {
                    append_field_byte(&mut field, b, limits.max_field_bytes)?;
                    index += 1;
                }
            },
            ParseState::Quoted => match byte {
                None => return Err(CsvError::UnterminatedQuotedField),
                Some(b'"') if bytes.get(index + 1) == Some(&b'"') => {
                    append_field_byte(&mut field, b'"', limits.max_field_bytes)?;
                    index += 2;
                }
                Some(b'"') => {
                    state = ParseState::AfterQuote;
                    index += 1;
                }
                Some(b) => {
                    append_field_byte(&mut field, b, limits.max_field_bytes)?;
                    index += 1;
                }
            },
            ParseState::AfterQuote => match byte {
                None => {
                    finish_field(&mut field, &mut record, &mut cells, limits)?;
                    finish_record(&mut record, &mut records, limits)?;
                    break;
                }
                Some(b) if b == limits.delimiter => {
                    finish_field(&mut field, &mut record, &mut cells, limits)?;
                    state = ParseState::StartField;
                    index += 1;
                }
                Some(b'\r') if bytes.get(index + 1) == Some(&b'\n') => {
                    finish_field(&mut field, &mut record, &mut cells, limits)?;
                    finish_record(&mut record, &mut records, limits)?;
                    state = ParseState::StartField;
                    index += 2;
                    if index == bytes.len() {
                        return Err(CsvError::TrailingRecordSeparator);
                    }
                }
                Some(b'\r' | b'\n') => return Err(CsvError::InvalidRecordSeparator),
                Some(_) => return Err(CsvError::UnexpectedAfterQuote),
            },
        }
    }
    Ok(records)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParseState {
    StartField,
    Unquoted,
    Quoted,
    AfterQuote,
}

fn valid_delimiter(delimiter: u8) -> bool {
    (0x20..=0x7e).contains(&delimiter) && delimiter != b'"'
}

fn validate_limits(limits: CsvLimits) -> Result<(), CsvError> {
    if !valid_delimiter(limits.delimiter)
        || limits.max_total_bytes == 0
        || limits.max_records == 0
        || limits.max_cells == 0
        || limits.max_fields_per_record == 0
        || limits.max_field_bytes == 0
        || limits.max_total_bytes > MAX_TOTAL_BYTES as usize
        || limits.max_records > MAX_RECORDS as usize
        || limits.max_cells > MAX_CELLS as usize
        || limits.max_fields_per_record > MAX_FIELDS_PER_RECORD as usize
        || limits.max_field_bytes > MAX_FIELD_BYTES as usize
    {
        return Err(CsvError::InvalidLimits);
    }
    Ok(())
}

fn append_str(output: &mut String, text: &str, maximum: usize) -> Result<(), CsvError> {
    let new_len = output
        .len()
        .checked_add(text.len())
        .ok_or(CsvError::EncodedTooLarge { maximum })?;
    if new_len > maximum {
        return Err(CsvError::EncodedTooLarge { maximum });
    }
    output
        .try_reserve(text.len())
        .map_err(|_| CsvError::AllocationFailed { requested: new_len })?;
    output.push_str(text);
    Ok(())
}

fn append_byte(output: &mut String, byte: u8, maximum: usize) -> Result<(), CsvError> {
    append_str(
        output,
        std::str::from_utf8(&[byte]).expect("dialect bytes are ASCII"),
        maximum,
    )
}

fn append_char(output: &mut String, ch: char, maximum: usize) -> Result<(), CsvError> {
    let new_len = output
        .len()
        .checked_add(ch.len_utf8())
        .ok_or(CsvError::EncodedTooLarge { maximum })?;
    if new_len > maximum {
        return Err(CsvError::EncodedTooLarge { maximum });
    }
    output
        .try_reserve(ch.len_utf8())
        .map_err(|_| CsvError::AllocationFailed { requested: new_len })?;
    output.push(ch);
    Ok(())
}

fn append_field_byte(field: &mut Vec<u8>, byte: u8, maximum: usize) -> Result<(), CsvError> {
    let new_len = field
        .len()
        .checked_add(1)
        .ok_or(CsvError::FieldTooLarge { maximum })?;
    if new_len > maximum {
        return Err(CsvError::FieldTooLarge { maximum });
    }
    field
        .try_reserve(1)
        .map_err(|_| CsvError::AllocationFailed { requested: new_len })?;
    field.push(byte);
    Ok(())
}

fn finish_field(
    field: &mut Vec<u8>,
    record: &mut Vec<String>,
    cells: &mut usize,
    limits: CsvLimits,
) -> Result<(), CsvError> {
    if record.len() >= limits.max_fields_per_record {
        return Err(CsvError::TooManyFields {
            maximum: limits.max_fields_per_record,
        });
    }
    let next_cells = cells.checked_add(1).ok_or(CsvError::TooManyCells {
        maximum: limits.max_cells,
    })?;
    if next_cells > limits.max_cells {
        return Err(CsvError::TooManyCells {
            maximum: limits.max_cells,
        });
    }
    let value = String::from_utf8(std::mem::take(field)).map_err(|_| CsvError::InvalidUtf8)?;
    record
        .try_reserve(1)
        .map_err(|_| CsvError::AllocationFailed {
            requested: record.len() + 1,
        })?;
    record.push(value);
    *cells = next_cells;
    Ok(())
}

fn finish_record(
    record: &mut Vec<String>,
    records: &mut Vec<Vec<String>>,
    limits: CsvLimits,
) -> Result<(), CsvError> {
    if records.len() >= limits.max_records {
        return Err(CsvError::TooManyRecords {
            maximum: limits.max_records,
        });
    }
    records
        .try_reserve(1)
        .map_err(|_| CsvError::AllocationFailed {
            requested: records.len() + 1,
        })?;
    records.push(std::mem::take(record));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declaration() -> Declaration {
        Declaration {
            delimiter: b',',
            max_total_bytes: 4096,
            max_records: 20,
            max_cells: 100,
            max_fields_per_record: 10,
            max_field_bytes: 512,
        }
    }

    fn host() -> HostProfile {
        HostProfile {
            pointer_width_bits: 64,
            standard_library: true,
            utf8_strings: true,
        }
    }

    fn limits() -> CsvLimits {
        declaration().normalize().unwrap().0
    }

    #[test]
    fn normalizes_bounds_and_fails_closed_for_delimiters_targets_and_host() {
        assert_eq!(
            Declaration {
                delimiter: b'"',
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::InvalidDelimiter)
        );
        assert_eq!(
            Declaration {
                delimiter: b'\n',
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::InvalidDelimiter)
        );
        assert_eq!(
            Declaration {
                max_cells: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ZeroLimit)
        );
        assert_eq!(
            Declaration {
                max_total_bytes: MAX_TOTAL_BYTES + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::LimitTooLarge)
        );
        let ir = declaration().normalize().unwrap();
        assert_eq!(
            ir.lower_checked(Target::Browser, host()),
            Err(Diagnostic::UnsupportedTarget)
        );
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocalRust,
                HostProfile {
                    pointer_width_bits: 16,
                    ..host()
                }
            ),
            Err(Diagnostic::UnsupportedPointerWidth)
        );
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocalRust,
                HostProfile {
                    standard_library: false,
                    ..host()
                }
            ),
            Err(Diagnostic::MissingStandardLibrary)
        );
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocalRust,
                HostProfile {
                    utf8_strings: false,
                    ..host()
                }
            ),
            Err(Diagnostic::MissingUtf8Strings)
        );
    }

    #[test]
    fn exact_dialect_round_trips_quotes_unicode_empty_fields_and_crlf() {
        let records = vec![
            vec!["plain".into(), "comma,here".into(), "say \"hi\"".into()],
            vec!["café".into(), "line1\r\nline2".into(), String::new()],
        ];
        let encoded = encode(&records, limits()).unwrap();
        assert_eq!(
            encoded,
            "plain,\"comma,here\",\"say \"\"hi\"\"\"\r\ncafé,\"line1\r\nline2\",\"\""
        );
        assert_eq!(decode(&encoded, limits()).unwrap(), records);
        assert_eq!(encode(&[], limits()).unwrap(), "");
        assert_eq!(decode("", limits()).unwrap(), Vec::<Vec<String>>::new());
    }

    #[test]
    fn deterministic_generated_utf8_records_round_trip_across_syntax_characters() {
        // Small deterministic generator: no random dependency or flaky seed.
        fn next(state: &mut u64) -> u64 {
            *state ^= *state << 13;
            *state ^= *state >> 7;
            *state ^= *state << 17;
            *state
        }

        let alphabet = [
            'a', 'Z', ',', ';', '"', '\r', '\n', ' ', 'é', 'λ', '🙂', '𝄞',
        ];
        let mut state = 0x4d595df4d0f33173;
        let mut records = Vec::new();
        records.push(vec![
            String::new(),
            ",".into(),
            "\"".into(),
            "\r\n".into(),
            "é🙂𝄞".into(),
        ]);
        for _ in 0..96 {
            let fields = (next(&mut state) % 8 + 1) as usize;
            let mut record = Vec::with_capacity(fields);
            for _ in 0..fields {
                let chars = (next(&mut state) % 32) as usize;
                let mut field = String::new();
                for _ in 0..chars {
                    let index = (next(&mut state) as usize) % alphabet.len();
                    field.push(alphabet[index]);
                }
                record.push(field);
            }
            records.push(record);
        }
        let limits = CsvLimits {
            max_total_bytes: 1024 * 1024,
            max_records: 128,
            max_cells: 1024,
            max_fields_per_record: 8,
            max_field_bytes: 1024,
            ..limits()
        };
        let encoded = encode(&records, limits).unwrap();
        assert_eq!(decode(&encoded, limits).unwrap(), records);
        assert_eq!(encode(&records, limits).unwrap(), encoded);

        let semicolon = CsvLimits {
            delimiter: b';',
            ..limits
        };
        let alternate_rows = vec![vec!["left;right".into(), "tail".into()]];
        let alternate = encode(&alternate_rows, semicolon).unwrap();
        assert_eq!(alternate, "\"left;right\";tail");
        assert_eq!(decode(&alternate, semicolon).unwrap(), alternate_rows);
    }

    #[test]
    fn exact_size_and_count_boundaries_are_accepted_and_next_byte_is_rejected() {
        let exact = CsvLimits {
            max_total_bytes: 3,
            max_records: 1,
            max_cells: 1,
            max_fields_per_record: 1,
            max_field_bytes: 3,
            ..limits()
        };
        let one = vec![vec!["abc".into()]];
        assert_eq!(encode(&one, exact).unwrap(), "abc");
        assert_eq!(decode("abc", exact).unwrap(), one);
        assert_eq!(
            encode(&[vec!["abcd".into()]], exact),
            Err(CsvError::FieldTooLarge { maximum: 3 })
        );
        assert_eq!(
            decode("abcd", exact),
            Err(CsvError::InputTooLarge { maximum: 3 })
        );
        assert_eq!(
            encode(&[vec!["a".into()], vec!["b".into()]], exact),
            Err(CsvError::TooManyRecords { maximum: 1 })
        );
        assert_eq!(
            encode(&[vec!["a".into(), "b".into()]], exact),
            Err(CsvError::TooManyFields { maximum: 1 })
        );
        assert_eq!(
            decode("a,b", exact),
            Err(CsvError::TooManyFields { maximum: 1 })
        );

        let quoted_exact = CsvLimits {
            max_total_bytes: 5,
            max_field_bytes: 3,
            ..exact
        };
        let quoted = vec![vec!["a,b".into()]];
        assert_eq!(encode(&quoted, quoted_exact).unwrap(), "\"a,b\"");
        assert_eq!(decode("\"a,b\"", quoted_exact).unwrap(), quoted);
        assert_eq!(
            encode(
                &quoted,
                CsvLimits {
                    max_total_bytes: 4,
                    ..quoted_exact
                }
            ),
            Err(CsvError::EncodedTooLarge { maximum: 4 })
        );
    }

    #[test]
    fn enforces_record_cell_field_and_total_byte_limits() {
        assert_eq!(
            encode(
                &[vec!["a".into(), "b".into()]],
                CsvLimits {
                    max_cells: 1,
                    ..limits()
                }
            ),
            Err(CsvError::TooManyCells { maximum: 1 })
        );
        assert_eq!(
            encode(
                &[vec!["a".into(), "b".into()]],
                CsvLimits {
                    max_fields_per_record: 1,
                    ..limits()
                }
            ),
            Err(CsvError::TooManyFields { maximum: 1 })
        );
        assert_eq!(
            encode(
                &[vec!["abc".into()]],
                CsvLimits {
                    max_field_bytes: 2,
                    ..limits()
                }
            ),
            Err(CsvError::FieldTooLarge { maximum: 2 })
        );
        assert_eq!(
            encode(
                &[vec!["abc".into()]],
                CsvLimits {
                    max_total_bytes: 2,
                    ..limits()
                }
            ),
            Err(CsvError::EncodedTooLarge { maximum: 2 })
        );
        assert_eq!(
            decode(
                "abc",
                CsvLimits {
                    max_total_bytes: 2,
                    ..limits()
                }
            ),
            Err(CsvError::InputTooLarge { maximum: 2 })
        );
        assert_eq!(encode(&[vec![]], limits()), Err(CsvError::EmptyRecord));
    }

    #[test]
    fn malformed_quotes_and_noncanonical_record_separators_are_rejected() {
        for (input, expected) in [
            ("ab\"cd", CsvError::MalformedQuote),
            ("\"open", CsvError::UnterminatedQuotedField),
            ("\"a\"\"b\"x", CsvError::UnexpectedAfterQuote),
            ("\"a\"\r", CsvError::InvalidRecordSeparator),
            ("\"a\"\n", CsvError::InvalidRecordSeparator),
            ("\"ok\"x", CsvError::UnexpectedAfterQuote),
            ("a\nb", CsvError::InvalidRecordSeparator),
            ("a\rb", CsvError::InvalidRecordSeparator),
            ("a\r\n", CsvError::TrailingRecordSeparator),
        ] {
            assert_eq!(decode(input, limits()), Err(expected), "input {input:?}");
        }
        assert_eq!(
            decode(
                "a,b\r\nc",
                CsvLimits {
                    max_records: 1,
                    ..limits()
                }
            ),
            Err(CsvError::TooManyRecords { maximum: 1 })
        );
    }

    #[test]
    fn generated_binding_compiles_and_runs_against_this_runtime() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let artifact = declaration()
            .normalize()
            .unwrap()
            .lower_checked(Target::ProcessLocalRust, host())
            .unwrap();
        let module = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/csv_caprichio.rs");
        let harness = format!(
            "#[path = {module:?}] mod csv_caprichio;\n{}\nfn main() {{\n\
             let rows = vec![vec![String::from(\"x,y\"), String::from(\"café\")], vec![String::new(), String::from(\"quote \\\"\")]];\n\
             let encoded = encode_csv(&rows).expect(\"encode\");\n\
             assert_eq!(decode_csv(&encoded).expect(\"decode\"), rows);\n\
             assert!(encode_csv(&[vec![String::from(\"z\"); 11]]).is_err());\n\
             }}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let binary =
            std::env::temp_dir().join(format!("goya-csv-factory-{}-{nonce}", std::process::id()));
        let mut compiler = Command::new("rustc")
            .args(["--edition=2021", "-o"])
            .arg(&binary)
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        compiler
            .stdin
            .take()
            .unwrap()
            .write_all(harness.as_bytes())
            .unwrap();
        let compile = compiler.wait_with_output().unwrap();
        assert!(
            compile.status.success(),
            "generated CSV binding did not compile:\n{}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let run = Command::new(&binary).output().unwrap();
        let _ = std::fs::remove_file(binary);
        assert!(
            run.status.success(),
            "generated CSV binding failed:\n{}",
            String::from_utf8_lossy(&run.stderr)
        );
    }

    /// Manual local construction/codec smoke benchmark; no throughput threshold.
    #[test]
    #[ignore = "manual CSV microbenchmark"]
    fn benchmark_csv_codec() {
        use std::time::Instant;
        let rows: Vec<Vec<String>> = (0..10_000)
            .map(|n| {
                vec![
                    format!("item-{n}"),
                    "plain value".into(),
                    "comma,value".into(),
                ]
            })
            .collect();
        let benchmark_limits = CsvLimits {
            max_total_bytes: MAX_TOTAL_BYTES as usize,
            max_records: MAX_RECORDS as usize,
            max_cells: MAX_CELLS as usize,
            max_fields_per_record: MAX_FIELDS_PER_RECORD as usize,
            max_field_bytes: MAX_FIELD_BYTES as usize,
            ..limits()
        };
        let start = Instant::now();
        for _ in 0..20 {
            let encoded = encode(&rows, benchmark_limits).unwrap();
            std::hint::black_box(decode(&encoded, benchmark_limits).unwrap());
        }
        eprintln!(
            "20 CSV encode/decode passes over 10,000 three-field records in {:?}",
            start.elapsed()
        );
    }
}
