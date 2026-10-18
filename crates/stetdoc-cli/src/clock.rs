//! The one place the CLI reads the clock: the default date of a stamp.

use jiff::Timestamp;
use jiff::tz::TimeZone;

/// The local wall-clock time as `YYYY-MM-DDTHH:MM:SS`, which is what the core's `Stamp` takes and what Word writes (its
/// `w:date` is the author's local time), with a warning when the date could not be local.
///
/// The zone is `TZ` when it is set (an IANA name such as `Asia/Kolkata`, a POSIX rule or a path to zone data), otherwise
/// the system's. Names resolve against the system's zone database, and against the copy built into the binary when the
/// system has none. A `TZ` that names no zone, or a system with no zone at all, gives UTC and a warning, so a wrong date is
/// never silent.
pub fn now_local() -> (String, Option<String>) {
    let (zone, warning) = match TimeZone::try_system() {
        Ok(zone) => (zone, None),
        Err(_) => {
            let warning = match std::env::var("TZ") {
                Ok(tz) => format!(
                    "warning: TZ={tz} is not a time zone stetdoc can read, so the date of these revisions is UTC. Set TZ to a zone such as Europe/London, or pass --date."
                ),
                Err(_) => "warning: the system's time zone could not be read, so the date of these revisions is UTC. Set TZ, or pass --date.".to_string(),
            };
            (TimeZone::UTC, Some(warning))
        }
    };
    let date = Timestamp::now()
        .to_zoned(zone)
        .strftime("%Y-%m-%dT%H:%M:%S")
        .to_string();
    (date, warning)
}
