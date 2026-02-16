/// Free-for-all scoring: score = number of tags scored.
pub fn ffa_score(tags_scored: u32) -> i32 {
    tags_scored as i32
}

/// Team scoring: team_score = sum of all members' tag counts.
pub fn team_score(member_tags: &[u32]) -> i32 {
    member_tags.iter().sum::<u32>() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffa_scoring() {
        assert_eq!(ffa_score(0), 0);
        assert_eq!(ffa_score(5), 5);
        assert_eq!(ffa_score(10), 10);
    }

    #[test]
    fn team_scoring() {
        assert_eq!(team_score(&[3, 2, 5]), 10);
        assert_eq!(team_score(&[0, 0]), 0);
        assert_eq!(team_score(&[1]), 1);
    }
}
