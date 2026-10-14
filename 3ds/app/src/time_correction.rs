// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

/// Preserve UTC precisely; infer local scheduling offset from the console
/// clock.
pub fn correction(local: i64, server: i64) -> Result<(i64, i32), &'static str> {
    let seconds = local.checked_sub(server).ok_or("Invalid clock response")?;
    if server < 1_704_067_200 || seconds.unsigned_abs() > 54_000 {
        return Err("Check the console date and time in System Settings");
    }
    let minutes = (((seconds.abs() + 450) / 900) * 15 * seconds.signum()) as i32;
    Ok((seconds, minutes.clamp(-840, 840)))
}

/// A saved correction expires after 30 minutes. Legacy corrections have no
/// verification timestamp and must be checked before network operations.
pub fn fresh(verified_local: Option<i64>, now: i64) -> bool {
    verified_local
        .and_then(|verified| now.checked_sub(verified))
        .is_some_and(|age| (0..1800).contains(&age))
}

#[cfg(test)]
mod tests {
    use super::*;
    const UTC: i64 = 1_800_000_000;
    #[test]
    fn east_west_and_fractional_offsets() {
        for minutes in [120, -300, 345, 840, 0] {
            assert_eq!(
                correction(UTC + minutes * 60, UTC),
                Ok((minutes * 60, minutes as i32))
            );
        }
    }
    #[test]
    fn clock_drift_is_corrected_without_changing_the_zone() {
        assert_eq!(correction(UTC + 7217, UTC), Ok((7217, 120)));
        assert_eq!(correction(UTC - 18017, UTC), Ok((-18017, -300)));
    }
    #[test]
    fn invalid_clock_is_rejected() {
        assert!(correction(UTC + 172800, UTC).is_err());
        assert!(correction(0, 0).is_err());
    }
    #[test]
    fn freshness_rejects_old_missing_and_backwards_clocks() {
        assert!(fresh(Some(UTC), UTC));
        assert!(fresh(Some(UTC), UTC + 1799));
        assert!(!fresh(Some(UTC), UTC + 1800));
        assert!(!fresh(Some(UTC), UTC - 1));
        assert!(!fresh(None, UTC));
        assert!(!fresh(Some(i64::MIN), i64::MAX));
    }
}
