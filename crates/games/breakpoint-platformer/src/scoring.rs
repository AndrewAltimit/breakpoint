use crate::combat::DEATH_TIME_PENALTY;

/// Calculate a player's score in Race mode with death penalty.
///
/// Scoring: 1st = 10, 2nd = 7, 3rd = 5, 4th = 4, 5th = 3, 6th = 2, rest = 1, DNF = 0.
/// Death penalty: subtract 0.5 per death (minimum final score of 0).
pub fn race_score(finish_position: Option<usize>, deaths: u8) -> i32 {
    let base = match finish_position {
        Some(0) => 10,
        Some(1) => 7,
        Some(2) => 5,
        Some(3) => 4,
        Some(4) => 3,
        Some(5) => 2,
        Some(_) => 1,
        None => 0,
    };
    let penalty = (deaths as f32 * 0.5).floor() as i32;
    (base - penalty).max(0)
}

/// Calculate the effective finish time including death time penalties.
///
/// Each death adds `DEATH_TIME_PENALTY` seconds to the actual finish time.
pub fn finish_time_with_penalty(actual_time: f32, deaths: u8) -> f32 {
    actual_time + deaths as f32 * DEATH_TIME_PENALTY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn race_positions_no_deaths() {
        assert_eq!(race_score(Some(0), 0), 10);
        assert_eq!(race_score(Some(1), 0), 7);
        assert_eq!(race_score(Some(2), 0), 5);
        assert_eq!(race_score(Some(5), 0), 2);
        assert_eq!(race_score(Some(10), 0), 1);
        assert_eq!(race_score(None, 0), 0);
    }

    #[test]
    fn race_score_with_deaths() {
        // 1st place with 2 deaths: 10 - 1 = 9
        assert_eq!(race_score(Some(0), 2), 9);
        // 1st place with 4 deaths: 10 - 2 = 8
        assert_eq!(race_score(Some(0), 4), 8);
        // 6th place with 6 deaths: 2 - 3 = clamped to 0
        assert_eq!(race_score(Some(5), 6), 0);
    }

    #[test]
    fn race_score_never_negative() {
        // Even with many deaths, score should not go below 0
        assert_eq!(race_score(Some(0), 100), 0);
        assert_eq!(race_score(None, 10), 0);
    }

    #[test]
    fn finish_time_penalty_calculation() {
        assert!((finish_time_with_penalty(60.0, 0) - 60.0).abs() < 0.001);
        assert!((finish_time_with_penalty(60.0, 2) - 66.0).abs() < 0.001);
        assert!((finish_time_with_penalty(90.0, 5) - 105.0).abs() < 0.001);
    }
}
