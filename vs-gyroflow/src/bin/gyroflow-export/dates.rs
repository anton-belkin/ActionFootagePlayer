// Times and dates given on the command line or read from the clip.
// One of gyroflow-export's parts: main.rs has the uses and main(); each part's items are the program's.

use super::*;

pub(crate) fn parse_time(s: &str) -> f64 {
    s.split(':').try_fold(0.0, |acc, part| part.parse::<f64>().map(|v| acc * 60.0 + v))
        .unwrap_or_else(|_| fail(&format!("bad time {s} (seconds, or m:ss)")))
}

// Days since 1970-01-01 of a date in the proleptic Gregorian calendar, and back (Howard Hinnant's algorithms)
pub(crate) fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + if m <= 2 { 1 } else { 0 }, m, d)
}

// The zone a DJI clip was recorded in, minutes from UTC: its name is the camera's local time
// (DJI_20261024094541_0093_D), its creation_time UTC (the workspace plugin's moments.lua does the same)
pub(crate) fn zone_of_name(stem: &str, created: f64) -> Option<i32> {
    let d = stem.strip_prefix("DJI_")?.get(..14)?;
    if !d.bytes().all(|b| b.is_ascii_digit()) { return None; }
    let n = |a: usize, b: usize| d[a..b].parse::<i64>().unwrap();
    let local = (days_from_civil(n(0, 4), n(4, 6), n(6, 8)) * 86400 + n(8, 10) * 3600 + n(10, 12) * 60 + n(12, 14)) as f64;
    let offset = ((local - created) / 900.0).round() as i32 * 15;
    (offset.abs() <= 14 * 60).then_some(offset)
}

// --dates: 2026-10-22T15:28:42[.123](Z|+02:00|+0200)
pub(crate) fn parse_dates(s: &str) -> Option<Dates> {
    let (date, time) = s.split_once(['T', ' '])?;
    let mut d = date.splitn(3, '-').map(|x| x.parse::<i64>().ok());
    let (y, mo, da) = (d.next()??, d.next()??, d.next()??);
    let (clock, offset_min) = if let Some(c) = time.strip_suffix(['Z', 'z']) { (c, 0) } else {
        let at = time.rfind(['+', '-'])?;
        let (c, zone) = time.split_at(at);
        let sign = if zone.starts_with('-') { -1 } else { 1 };
        let digits: String = zone[1..].chars().filter(|c| *c != ':').collect();
        if digits.len() != 4 { return None; }
        let (h, m) = (digits[..2].parse::<i32>().ok()?, digits[2..].parse::<i32>().ok()?);
        (c, sign * (h * 60 + m))
    };
    let mut t = clock.splitn(3, ':');
    let (h, mi, sec) = (t.next()?.parse::<i64>().ok()?, t.next()?.parse::<i64>().ok()?, t.next()?.parse::<f64>().ok()?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&da) || !(0..24).contains(&h) || !(0..60).contains(&mi) || !(0.0..61.0).contains(&sec) { return None; }
    let local = (days_from_civil(y, mo, da) * 86400 + h * 3600 + mi * 60) as f64 + sec;
    Some(Dates { unix: local - offset_min as f64 * 60.0, offset_min })
}

impl Dates {
    // The time in its zone, or in UTC: (y, m, d, h, min, s, ms)
    fn fields(&self, offset_min: i32) -> (i64, i64, i64, i64, i64, i64, i64) {
        let ms = ((self.unix + offset_min as f64 * 60.0) * 1000.0).round() as i64;
        let (secs, ms) = (ms.div_euclid(1000), ms.rem_euclid(1000));
        let (y, m, d) = civil_from_days(secs.div_euclid(86400));
        let s = secs.rem_euclid(86400);
        (y, m, d, s / 3600, s / 60 % 60, s % 60, ms)
    }
    // The MP4's creation_time, for ffmpeg: UTC
    pub(crate) fn utc(&self) -> String {
        let (y, m, d, h, mi, s, ms) = self.fields(0);
        format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}.{ms:03}000Z")
    }
    // QuickTime's creation date, as Apple's cameras write it: local time and its offset
    pub(crate) fn quicktime(&self) -> String {
        let (y, m, d, h, mi, s, _) = self.fields(self.offset_min);
        let o = self.offset_min.abs();
        format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}{}{:02}{:02}", if self.offset_min < 0 { '-' } else { '+' }, o / 60, o % 60)
    }
    pub(crate) fn system_time(&self) -> std::time::SystemTime {
        let d = std::time::Duration::from_secs_f64(self.unix.abs());
        if self.unix >= 0.0 { std::time::UNIX_EPOCH + d } else { std::time::UNIX_EPOCH - d }
    }
}
