//! Scryfall bulk-data metadata and JSON Lines decoding.
//!
//! Scryfall publishes its card database as gzip-compressed JSON Lines. The
//! file is large (a few GB uncompressed), so [`JsonLines`] decodes it one
//! record at a time from any reader instead of loading the whole payload.

use std::io::{self, BufRead, BufReader, Read};
use std::marker::PhantomData;

use flate2::bufread::GzDecoder;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// The bulk-data listing endpoint.
pub const BULK_DATA_URL: &str = "https://api.scryfall.com/bulk-data";

/// The metadata endpoint for the `default_cards` bulk file.
pub const DEFAULT_CARDS_URL: &str = "https://api.scryfall.com/bulk-data/default-cards";

/// The gzip magic number.
const GZIP_MAGIC: [u8; 2] = [0x1F, 0x8B];

/// Metadata for one bulk-data file.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BulkData {
    /// The bulk type, such as `default_cards` or `oracle_cards`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Download URI for the gzip JSON Lines file. Scryfall no longer sets
    /// `download_uri`; `jsonl_download_uri` is the only one served.
    #[serde(default)]
    pub jsonl_download_uri: Option<String>,
    /// When the file was generated.
    #[serde(default, deserialize_with = "lenient_datetime")]
    pub updated_at: Option<OffsetDateTime>,
    /// Compressed size in bytes.
    #[serde(default)]
    pub size: Option<u64>,
}

/// The bulk-data listing (`GET /bulk-data`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BulkDataList {
    /// Every bulk file Scryfall offers.
    #[serde(default)]
    pub data: Vec<BulkData>,
}

impl BulkDataList {
    /// The entry with the given `type`.
    #[must_use]
    pub fn find(&self, kind: &str) -> Option<&BulkData> {
        self.data.iter().find(|entry| entry.kind == kind)
    }
}

impl BulkData {
    /// The JSON Lines download URI, or an error naming what was missing.
    pub fn download_uri(&self) -> Result<&str, BulkError> {
        self.jsonl_download_uri
            .as_deref()
            .filter(|uri| !uri.is_empty())
            .ok_or(BulkError::MissingDownloadUri)
    }
}

fn lenient_datetime<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<OffsetDateTime>, D::Error> {
    let raw: Option<String> = Option::deserialize(deserializer)?;
    Ok(raw
        .as_deref()
        .and_then(|value| OffsetDateTime::parse(value, &Rfc3339).ok())
        .map(|value| value.replace_nanosecond(0).unwrap_or(value)))
}

/// Why a bulk file could not be used.
#[derive(Debug, thiserror::Error)]
pub enum BulkError {
    /// The metadata did not carry a JSON Lines download URI.
    #[error("Scryfall bulk metadata did not include jsonl_download_uri")]
    MissingDownloadUri,
    /// The payload did not start with the gzip magic number.
    #[error("Scryfall bulk payload was not gzip-compressed JSON Lines")]
    NotGzip,
    /// Reading or decompressing the payload failed. A truncated gzip stream
    /// surfaces here as an unexpected end of file.
    #[error("Could not decompress Scryfall bulk payload: {0}")]
    Io(#[from] io::Error),
    /// A line was not the expected JSON record.
    #[error("Invalid Scryfall JSON Lines record on line {line}: {source}")]
    Json {
        /// 1-based line number.
        line: usize,
        /// The decode error.
        #[source]
        source: serde_json::Error,
    },
}

/// Whether `bytes` begin a gzip stream.
#[must_use]
pub fn is_gzip(bytes: &[u8]) -> bool {
    bytes.starts_with(&GZIP_MAGIC)
}

/// An iterator of records decoded from JSON Lines.
///
/// Blank lines are skipped and a trailing `\r` is trimmed. Each record is
/// decoded independently, so a caller can stop early without reading the
/// whole stream. A gzip stream that ends early produces an error on the
/// record that was cut off, as the Elixir importers did.
pub struct JsonLines<R, T> {
    reader: R,
    line: usize,
    buffer: String,
    finished: bool,
    record: PhantomData<fn() -> T>,
}

impl<T: DeserializeOwned> JsonLines<BufReader<GzDecoder<BufReader<Box<dyn Read + Send>>>>, T> {
    /// Streams records from a gzip-compressed JSON Lines reader.
    pub fn gzip<R: Read + Send + 'static>(reader: R) -> Self {
        let boxed: Box<dyn Read + Send> = Box::new(reader);
        Self::new(BufReader::new(GzDecoder::new(BufReader::new(boxed))))
    }
}

impl<T: DeserializeOwned> JsonLines<BufReader<GzDecoder<BufReader<io::Cursor<Vec<u8>>>>>, T> {
    /// Streams records from an in-memory gzip payload, checking the gzip
    /// magic number first.
    pub fn gzip_bytes(bytes: Vec<u8>) -> Result<Self, BulkError> {
        if !is_gzip(&bytes) {
            return Err(BulkError::NotGzip);
        }
        Ok(Self::new(BufReader::new(GzDecoder::new(BufReader::new(
            io::Cursor::new(bytes),
        )))))
    }
}

impl<R: BufRead, T: DeserializeOwned> JsonLines<R, T> {
    /// Streams records from an uncompressed JSON Lines reader.
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            line: 0,
            buffer: String::new(),
            finished: false,
            record: PhantomData,
        }
    }

    /// The number of lines read so far, blank lines included.
    #[must_use]
    pub fn lines_read(&self) -> usize {
        self.line
    }
}

impl<R: BufRead, T: DeserializeOwned> Iterator for JsonLines<R, T> {
    type Item = Result<T, BulkError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.finished {
                return None;
            }
            self.buffer.clear();
            match self.reader.read_line(&mut self.buffer) {
                Ok(0) => {
                    self.finished = true;
                    return None;
                }
                Ok(_) => {}
                Err(error) => {
                    self.finished = true;
                    return Some(Err(BulkError::Io(error)));
                }
            }
            self.line += 1;
            let text = self.buffer.trim_end_matches(['\n', '\r']);
            if text.trim().is_empty() {
                continue;
            }
            return Some(
                serde_json::from_str(text).map_err(|source| BulkError::Json {
                    line: self.line,
                    source,
                }),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use flate2::Compression;
    use flate2::write::GzEncoder;
    use serde_json::{Value, json};

    use super::*;

    fn gzip_jsonl(records: &[Value]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        for record in records {
            serde_json::to_writer(&mut encoder, record).unwrap();
            encoder.write_all(b"\n").unwrap();
        }
        encoder.finish().unwrap()
    }

    #[test]
    fn decodes_a_payload_larger_than_one_inflate_chunk() {
        let records: Vec<Value> = (1..=3000)
            .map(|index| json!({"name": format!("Card {index}"), "digest": "x".repeat(64)}))
            .collect();
        let uncompressed: usize = records
            .iter()
            .map(|record| record.to_string().len() + 1)
            .sum();
        assert!(
            uncompressed > 64 * 1024,
            "{uncompressed} bytes before compression"
        );
        let payload = gzip_jsonl(&records);
        let decoded: Vec<Value> = JsonLines::gzip_bytes(payload)
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(decoded.len(), 3000);
        assert_eq!(decoded[0]["name"], "Card 1");
        assert_eq!(decoded[2999]["name"], "Card 3000");
    }

    #[test]
    fn decoding_can_stop_early() {
        let records: Vec<Value> = (1..=3000).map(|i| json!({"n": i})).collect();
        let mut lines = JsonLines::<_, Value>::gzip_bytes(gzip_jsonl(&records)).unwrap();
        let first = lines.next().unwrap().unwrap();
        assert_eq!(first["n"], 1);
        assert_eq!(lines.lines_read(), 1);
    }

    #[test]
    fn truncated_gzip_is_rejected() {
        let payload = gzip_jsonl(&[json!({"name": "Incomplete"})]);
        let truncated = payload[..payload.len() - 8].to_vec();
        let results: Vec<Result<Value, BulkError>> =
            JsonLines::gzip_bytes(truncated).unwrap().collect();
        assert!(
            results
                .iter()
                .any(|result| matches!(result, Err(BulkError::Io(_)))),
            "{results:?}"
        );
    }

    #[test]
    fn rejects_non_gzip_payloads() {
        assert!(matches!(
            JsonLines::<_, Value>::gzip_bytes(b"[]".to_vec()),
            Err(BulkError::NotGzip)
        ));
    }

    #[test]
    fn skips_blank_lines_and_trims_carriage_returns() {
        let input = "{\"a\":1}\r\n\n  \n{\"a\":2}";
        let decoded: Vec<Value> = JsonLines::new(input.as_bytes())
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(decoded, vec![json!({"a": 1}), json!({"a": 2})]);
    }

    #[test]
    fn reports_the_line_of_a_bad_record() {
        let input = "{\"a\":1}\nnot json\n";
        let results: Vec<Result<Value, BulkError>> = JsonLines::new(input.as_bytes()).collect();
        assert!(matches!(results[1], Err(BulkError::Json { line: 2, .. })));
        let input = "[1]\n";
        let results: Vec<Result<serde_json::Map<String, Value>, BulkError>> =
            JsonLines::new(input.as_bytes()).collect();
        assert!(matches!(results[0], Err(BulkError::Json { line: 1, .. })));
    }

    #[test]
    fn bulk_metadata_parses_timestamps_and_finds_default_cards() {
        let list: BulkDataList = serde_json::from_str(
            r#"{"data":[{"type":"oracle_cards","download_uri":null},
                {"type":"default_cards","jsonl_download_uri":"https://data.scryfall.io/default-cards/x.jsonl.gz","updated_at":"2026-10-06T09:05:12.123+00:00","size":5}]}"#,
        )
        .unwrap();
        let default_cards = list.find("default_cards").unwrap();
        assert_eq!(
            default_cards.download_uri().unwrap(),
            "https://data.scryfall.io/default-cards/x.jsonl.gz"
        );
        assert_eq!(
            default_cards.updated_at,
            Some(time::macros::datetime!(2026-10-06 09:05:12 UTC))
        );
        assert!(matches!(
            list.find("oracle_cards").unwrap().download_uri(),
            Err(BulkError::MissingDownloadUri)
        ));
        assert!(list.find("rulings").is_none());
    }
}
