//use crate::rpcvalue::RpcValue;

use std::cmp::Ordering;
use std::fmt;
use chrono::{FixedOffset, NaiveDateTime, Offset};

/// msec: 57, tz: 7;
/// tz is stored as signed count of quarters of hour (15 min)
/// I'm storing whole DateTime in one i64 to keep size_of RpcValue == 24
const TZ_MASK: i64 = 127;
pub enum IncludeMilliseconds {
    #[allow(dead_code)]
    Never,
    Always,
    WhenNonZero,
}
pub struct ToISOStringOptions {
    pub(crate) include_millis: IncludeMilliseconds,
    pub(crate) include_timezone: bool,
}
impl Default for ToISOStringOptions {
    fn default() -> Self {
        ToISOStringOptions {
            include_millis: IncludeMilliseconds::Always,
            include_timezone: true
        }
    }
}
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct DateTime (i64);
impl DateTime {
    //pub fn invalid() -> DateTime {
    //    DateTime::from_epoch_msec(0)
    //}
    //pub fn is_valid(&self) -> bool { }
    pub fn now() -> DateTime {
        let dt = chrono::offset::Local::now();
        let msec = dt.naive_utc().and_utc().timestamp_millis();
        let offset = dt.offset().local_minus_utc() / 60 / 15;
        DateTime::from_epoch_msec_tz(msec, offset)
    }

    pub fn from_datetime<Tz: chrono::TimeZone>(dt: &chrono::DateTime<Tz>) -> DateTime {
        let msec = dt.naive_utc().and_utc().timestamp_millis();
        let offset = dt.offset().fix().local_minus_utc();
        DateTime::from_epoch_msec_tz(msec, offset)
    }
    pub fn from_naive_datetime(dt: &chrono::NaiveDateTime) -> DateTime {
        let msec = dt.and_utc().timestamp_millis();
        DateTime::from_epoch_msec(msec)
    }
    pub fn from_epoch_msec_tz(epoch_msec: i64, utc_offset_sec: i32) -> DateTime {
        let mut msec = epoch_msec;
        // offset in quarters of hour
        msec *= TZ_MASK + 1;
        let offset = (utc_offset_sec / 60 / 15) as i64;
        msec |= offset & TZ_MASK;
        DateTime(msec)
    }
    pub fn from_epoch_msec(epoch_msec: i64) -> DateTime {
        Self::from_epoch_msec_tz(epoch_msec, 0)
    }
    pub fn from_iso_str(iso_str: &str) -> Result<DateTime, String> {
            const PATTERN: &str = "2020-02-03T11:59:43";
            if iso_str.len() >= PATTERN.len() {
                let s = iso_str;
                let naive_str = &s[..PATTERN.len()];
                if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(naive_str, "%Y-%m-%dT%H:%M:%S") {
                    let mut msec = 0;
                    let mut offset = 0;
                    let mut rest = &s[PATTERN.len()..];
                    if !rest.is_empty() && rest.as_bytes()[0] == b'.' {
                        rest = &rest[1..];
                        if rest.len() >= 3 {
                            match rest[..3].parse::<i32>() {
                                Ok(ms) => {
                                    msec = ms;
                                    rest = &rest[3..];
                                }
                                Err(err) => {
                                    return Err(format!("Parsing DateTime msec part error: {}, in '{}", err, iso_str))
                                }
                            }
                        }
                    }
                    if !rest.is_empty() {
                        if rest.len() == 1 && rest.as_bytes()[0] == b'Z' {
                        } else if rest.len() == 3 {
                            if let Ok(hrs) = rest.parse::<i32>() {
                                offset = 60 * 60 * hrs;
                            } else {
                                return Err(format!("Invalid DateTime TZ(3) part: '{}, date time: {}", rest, iso_str))
                            }
                        } else if rest.len() == 5 {
                            if let Ok(hrs) = rest.parse::<i32>() {
                                offset = 60 * (60 * (hrs / 100) + (hrs % 100));
                            } else {
                                return Err(format!("Invalid DateTime TZ(5) part: '{}, date time: {}", rest, iso_str))
                            }
                        } else {
                            return Err(format!("Invalid DateTime TZ part: '{}, date time: {}", rest, iso_str))
                        }
                    }

                    let dt = DateTime::from_epoch_msec_tz((ndt.and_utc().timestamp() - (offset as i64)) * 1000 + (msec as i64), offset);
                    return Ok(dt)
                }
            }
            Err(format!("Invalid DateTime: '{:?}", iso_str))
    }
    pub fn epoc_msec_utc_offset(&self) -> (i64, i32) {
        let msec= self.0 / (TZ_MASK + 1);
        let mut offset = self.0 & TZ_MASK;
        if (offset & ((TZ_MASK + 1) / 2)) != 0 {
            // sign extension
            offset |= !TZ_MASK;
        }
        let offset = (offset * 15 * 60) as i32;
        (msec, offset)
    }
    pub fn epoch_msec(&self) -> i64 { self.epoc_msec_utc_offset().0 }
    pub fn utc_offset(&self) -> i32 { self.epoc_msec_utc_offset().1 }

    pub fn to_chrono_naivedatetime(&self) -> chrono::NaiveDateTime {
        let msec = self.epoch_msec();
        chrono::DateTime::from_timestamp_millis(msec).unwrap_or_default().naive_utc()
    }
    pub fn to_chrono_datetime(&self) -> chrono::DateTime<chrono::offset::FixedOffset> {
        let offset = match FixedOffset::east_opt(self.utc_offset()) {
            None => {FixedOffset::east_opt(0).unwrap()}
            Some(o) => {o}
        };
        chrono::DateTime::from_naive_utc_and_offset(self.to_chrono_naivedatetime(), offset)
    }
    pub fn to_iso_string(&self) -> String {
        self.to_iso_string_opt(&ToISOStringOptions::default())
    }
    pub fn to_iso_string_opt(&self, opts: &ToISOStringOptions) -> String {
        let dt = self.to_chrono_datetime();
        let mut s = format!("{}", dt.format("%Y-%m-%dT%H:%M:%S"));
        let ms = self.epoch_msec() % 1000;
        match opts.include_millis {
            IncludeMilliseconds::Never => {}
            IncludeMilliseconds::Always => { s.push_str(&format!(".{:03}", ms)); }
            IncludeMilliseconds::WhenNonZero => {
                if ms > 0 {
                    s.push_str(&format!(".{:03}", ms));
                }
            }
        }
        if opts.include_timezone {
            let mut offset = self.utc_offset();
            if offset == 0 {
                s.push('Z');
            }
            else {
                if offset < 0 {
                    s.push('-');
                    offset = -offset;
                } else {
                    s.push('+');
                }
                let offset_hr = offset / 60 / 60;
                let offset_min = offset / 60 % 60;
                s += &format!("{:02}", offset_hr);
                if offset_min > 0 {
                    s += &format!("{:02}", offset_min);
                }
            }
        }
        s
    }

    pub fn add_days(&self, days: i64) -> Self {
        let (msec, offset) = self.epoc_msec_utc_offset();
        Self::from_epoch_msec_tz(msec + (days * 24 * 60 * 60 * 1000), offset)
    }
    pub fn add_hours(&self, hours: i64) -> Self {
        let (msec, offset) = self.epoc_msec_utc_offset();
        Self::from_epoch_msec_tz(msec + (hours * 60 * 60 * 1000), offset)
    }
    pub fn add_minutes(&self, minutes: i64) -> Self {
        let (msec, offset) = self.epoc_msec_utc_offset();
        Self::from_epoch_msec_tz(msec + (minutes * 60 * 1000), offset)
    }
    pub fn add_seconds(&self, seconds: i64) -> Self {
        let (msec, offset) = self.epoc_msec_utc_offset();
        Self::from_epoch_msec_tz(msec + (seconds * 1000), offset)
    }
    pub fn add_millis(&self, millis: i64) -> Self {
        let (msec, offset) = self.epoc_msec_utc_offset();
        Self::from_epoch_msec_tz(msec + millis, offset)
    }
}

impl PartialOrd for DateTime {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for DateTime {}

impl Ord for DateTime {
    fn cmp(&self, other: &Self) -> Ordering {
        let e1 = self.epoch_msec();
        let e2 = other.epoch_msec();
        e1.cmp(&e2)
    }
}

impl fmt::Display for DateTime {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_iso_string())
    }
}

impl From<NaiveDateTime> for DateTime {
    fn from(ndt: NaiveDateTime) -> Self {
        DateTime::from_naive_datetime(&ndt)
    }
}
