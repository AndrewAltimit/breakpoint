/// Points awarded for surviving the round.
pub const SURVIVE_POINTS: i32 = 10;
/// Points awarded per kill (opponent hits your wall).
pub const KILL_POINTS: i32 = 3;
/// Points deducted for dying.
pub const DEATH_POINTS: i32 = -2;
/// Points deducted for suicide (hitting your own wall).
pub const SUICIDE_POINTS: i32 = -4;

/// Calculate a player's score for a round.
pub fn calculate_score(survived: bool, kills: u32, died: bool, suicide: bool) -> i32 {
    let mut score = 0;
    if survived {
        score += SURVIVE_POINTS;
    }
    score += kills as i32 * KILL_POINTS;
    if died {
        if suicide {
            score += SUICIDE_POINTS;
        } else {
            score += DEATH_POINTS;
        }
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn survivor_with_kills() {
        assert_eq!(calculate_score(true, 3, false, false), 10 + 9);
    }

    #[test]
    fn died_to_enemy() {
        assert_eq!(calculate_score(false, 0, true, false), -2);
    }

    #[test]
    fn suicide_penalty() {
        assert_eq!(calculate_score(false, 0, true, true), -4);
    }

    #[test]
    fn no_events() {
        assert_eq!(calculate_score(false, 0, false, false), 0);
    }
}
