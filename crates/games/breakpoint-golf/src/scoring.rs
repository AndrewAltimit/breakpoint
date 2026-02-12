/// Calculate a player's score for a completed hole.
///
/// Scoring rules:
/// - First to sink: +3 bonus
/// - Under par: +2 per stroke under
/// - At par: +1
/// - Over par: 0
/// - DNF (did not finish): -1
pub fn calculate_score(strokes: u32, par: u8, was_first_sink: bool, finished: bool) -> i32 {
    if !finished {
        return -1;
    }

    let par = par as i32;
    let strokes = strokes as i32;

    let mut score = if strokes < par {
        // Under par: +2 per stroke under
        (par - strokes) * 2
    } else if strokes == par {
        1
    } else {
        0
    };

    if was_first_sink {
        score += 3;
    }

    score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_sink_under_par() {
        // Par 3, 1 stroke, first to sink: (3-1)*2 + 3 = 7
        assert_eq!(calculate_score(1, 3, true, true), 7);
    }

    #[test]
    fn second_sink_under_par() {
        // Par 3, 2 strokes, not first: (3-2)*2 = 2
        assert_eq!(calculate_score(2, 3, false, true), 2);
    }

    #[test]
    fn at_par_first_sink() {
        // Par 3, 3 strokes, first: 1 + 3 = 4
        assert_eq!(calculate_score(3, 3, true, true), 4);
    }

    #[test]
    fn at_par_not_first() {
        // Par 3, 3 strokes, not first: 1
        assert_eq!(calculate_score(3, 3, false, true), 1);
    }

    #[test]
    fn over_par() {
        // Par 3, 5 strokes: 0
        assert_eq!(calculate_score(5, 3, false, true), 0);
    }

    #[test]
    fn over_par_first_sink() {
        // Par 3, 4 strokes, first: 0 + 3 = 3
        assert_eq!(calculate_score(4, 3, true, true), 3);
    }

    #[test]
    fn dnf() {
        assert_eq!(calculate_score(0, 3, false, false), -1);
    }

    #[test]
    fn dnf_ignores_first_flag() {
        assert_eq!(calculate_score(0, 3, true, false), -1);
    }

    #[test]
    fn hole_in_one_first() {
        // Par 3, 1 stroke, first: (3-1)*2 + 3 = 7
        assert_eq!(calculate_score(1, 3, true, true), 7);
    }

    #[test]
    fn hole_in_one_not_first() {
        // Par 3, 1 stroke, not first: (3-1)*2 = 4
        assert_eq!(calculate_score(1, 3, false, true), 4);
    }
}
