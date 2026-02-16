/// Calculate a player's score in Race mode.
///
/// Scoring: 1st = 10, 2nd = 7, 3rd = 5, 4th = 4, 5th = 3, 6th = 2, rest = 1, DNF = 0.
pub fn race_score(finish_position: Option<usize>) -> i32 {
    match finish_position {
        Some(0) => 10,
        Some(1) => 7,
        Some(2) => 5,
        Some(3) => 4,
        Some(4) => 3,
        Some(5) => 2,
        Some(_) => 1,
        None => 0,
    }
}

/// Calculate a player's score in Survival mode.
///
/// Scoring: last alive = N points (N = total players), first eliminated = 1 point.
pub fn survival_score(elimination_order: Option<usize>, total_players: usize) -> i32 {
    match elimination_order {
        Some(order) => (total_players - order) as i32,
        None => total_players as i32, // survived to the end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn race_positions() {
        assert_eq!(race_score(Some(0)), 10);
        assert_eq!(race_score(Some(1)), 7);
        assert_eq!(race_score(Some(2)), 5);
        assert_eq!(race_score(Some(5)), 2);
        assert_eq!(race_score(Some(10)), 1);
        assert_eq!(race_score(None), 0);
    }

    #[test]
    fn survival_scoring() {
        // 4 players: last alive gets 4 pts, first eliminated gets 3 pts
        assert_eq!(survival_score(None, 4), 4); // survived
        assert_eq!(survival_score(Some(0), 4), 4); // eliminated first
        assert_eq!(survival_score(Some(3), 4), 1); // eliminated last
    }
}
