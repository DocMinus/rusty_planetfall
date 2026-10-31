//! The `random` opcode's generator (Standard 2.4): normally unpredictable,
//! but a story (or a test) can switch it into predictable mode.

use std::time::{SystemTime, UNIX_EPOCH};

pub struct Rng {
    state: u64,
    /// Predictable mode with a small seed counts 1, 2, ..., seed, 1, 2, ...
    counter: Option<(u16, u16)>,
}

impl Rng {
    pub fn new() -> Self {
        let mut rng = Self { state: 0, counter: None };
        rng.reseed_random();
        rng
    }

    pub fn reseed_random(&mut self) {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
        self.seed(nanos);
    }

    pub fn seed(&mut self, seed: u64) {
        self.state = seed ^ 0x9E37_79B9_7F4A_7C15;
        if self.state == 0 {
            self.state = 1;
        }
        self.counter = None;
    }

    /// `random` with a negative or zero argument.
    pub fn seed_predictable(&mut self, seed: u16) {
        if seed < 1000 {
            self.counter = Some((0, seed.max(1)));
        } else {
            self.seed(seed as u64);
        }
    }

    /// A number in 1..=range.
    pub fn next(&mut self, range: u16) -> u16 {
        if let Some((n, limit)) = &mut self.counter {
            *n = *n % *limit + 1;
            return (*n - 1) % range + 1;
        }
        // xorshift64*
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        let x = self.state.wrapping_mul(0x2545_F491_4F6C_DD1D);
        ((x >> 32) % range as u64) as u16 + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stays_in_range() {
        let mut r = Rng::new();
        for _ in 0..1000 {
            assert!((1..=6).contains(&r.next(6)));
        }
    }

    #[test]
    fn predictable_counts_up() {
        let mut r = Rng::new();
        r.seed_predictable(3);
        let seq: Vec<u16> = (0..6).map(|_| r.next(10)).collect();
        assert_eq!(seq, [1, 2, 3, 1, 2, 3]);
    }

    #[test]
    fn same_seed_same_sequence() {
        let (mut a, mut b) = (Rng::new(), Rng::new());
        a.seed(1234);
        b.seed(1234);
        assert!((0..20).all(|_| a.next(100) == b.next(100)));
    }
}
