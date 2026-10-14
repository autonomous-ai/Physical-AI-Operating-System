//! Provides custom serialization for schema objects.

use jiff::Timestamp;
use serde::Serializer;
use std::fmt;

/// Serializes a [`Timestamp`] using 0, 3, 6, or 9 fractional second digits,
/// choosing the fewest digits that still represent the value losslessly.
///
/// This matches the behavior of `chrono`'s serde implementation
/// (`SecondsFormat::AutoSi`), which `tough` used before migrating to `jiff`.
/// By default `jiff` normalizes away trailing zeros in fractional seconds, so
/// re-serialized metadata bytes could differ from the signed bytes and fail
/// signature verification.
///
/// See <https://github.com/awslabs/tough/issues/950>.
pub(super) fn serialize_timestamp<S: Serializer>(
    timestamp: &Timestamp,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    struct TimestampDisplay<'a>(&'a Timestamp);

    impl fmt::Display for TimestampDisplay<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let nanos = self.0.subsec_nanosecond();
            let precision: usize = if nanos == 0 {
                0
            } else if nanos % 1_000_000 == 0 {
                3
            } else if nanos % 1_000 == 0 {
                6
            } else {
                9
            };
            write!(f, "{:.*}", precision, self.0)
        }
    }

    serializer.collect_str(&TimestampDisplay(timestamp))
}

#[cfg(test)]
mod tests {
    use crate::schema::Timestamp;

    /// Re-serializing an `expires` timestamp must reproduce the exact bytes
    /// that were signed, for every precision `chrono` could have emitted --
    /// including fractional seconds with trailing zeros (#950).
    #[test]
    fn expires_roundtrips_chrono_compatible_precision() {
        for expires in [
            "2026-07-13T18:08:28Z",
            "2026-07-13T18:08:28.756Z",
            "2026-07-13T18:08:28.750Z",
            "2026-07-13T18:08:28.756787Z",
            "2026-07-13T18:08:28.756780Z",
            "2026-07-13T18:08:28.756787215Z",
            "2026-07-13T18:08:28.756787210Z",
        ] {
            let json = format!(
                r#"{{"_type":"timestamp","spec_version":"1.0.0","version":1,"expires":"{expires}","meta":{{}}}}"#
            );
            let role: Timestamp = serde_json::from_str(&json).unwrap();
            let value = serde_json::to_value(&role).unwrap();
            assert_eq!(value["expires"], expires, "failed to roundtrip {expires}");
        }
    }
}
