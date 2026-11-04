//! Match rules: lives, respawn countdown, spawn protection, win / lose.
//!
//! The match does not own the ships: it is given them each update, together
//! with the places a ship may respawn and a random number generator, and
//! it tells the caller what happened (the returned events) so effects can
//! react.

use crate::constants::*;
use crate::random::Rng;
use crate::ship::Ship;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    /// Flying (maybe still protected)
    Alive,
    /// At the spawn point, frozen, counting down
    Countdown,
    /// Destroyed, waiting to respawn
    Dead,
    /// No lives left
    Out,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    Victory,
    Defeat,
    Draw,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MatchEvent {
    /// This player's ship was just destroyed
    Died(usize),
    /// This player's ship has just reappeared (and starts counting down)
    Respawned(usize),
}

pub struct PlayerState {
    pub lives: u32,
    pub phase: Phase,
    /// Seconds left in Countdown / Dead
    timer: f64,
    /// Seconds left of the "GO!" message
    go_timer: f64,
}

pub struct Match {
    /// Lives each player began with
    pub starting_lives: u32,
    pub players: Vec<PlayerState>,
    pub over: bool,
    /// Seconds since the match ended
    pub over_time: f64,
    /// The winning player; None while playing, or after a draw
    pub winner: Option<usize>,
}

impl Match {
    /// Start a match: every ship counts down before it may move
    pub fn new(ships: &mut [Ship], lives: u32) -> Self {
        let mut m = Match {
            starting_lives: lives,
            players: ships.iter().map(|_| PlayerState {
                lives, phase: Phase::Alive, timer: 0.0, go_timer: 0.0,
            }).collect(),
            over: false,
            over_time: 0.0,
            winner: None,
        };
        for player in 0..ships.len() {
            m.begin_countdown(ships, player);
        }
        m
    }

    // -------------------------
    // Queries (for the HUD / overlays)
    // -------------------------
    pub fn lives(&self, player: usize) -> u32 {
        self.players[player].lives
    }

    /// 3, 2 or 1 while that player is counting down, else None
    pub fn countdown_number(&self, player: usize) -> Option<u32> {
        let p = &self.players[player];
        if p.phase != Phase::Countdown {
            return None;
        }
        Some((p.timer.ceil() as u32).clamp(1, COUNTDOWN_SECONDS as u32))
    }

    pub fn show_go(&self, player: usize) -> bool {
        self.players[player].go_timer > 0.0
    }

    /// Victory / Defeat / Draw once the match is over, else None
    pub fn outcome(&self, player: usize) -> Option<Outcome> {
        if !self.over {
            return None;
        }
        Some(match self.winner {
            None => Outcome::Draw,
            Some(w) if w == player => Outcome::Victory,
            Some(_) => Outcome::Defeat,
        })
    }

    /// Time to show the game-over menu
    pub fn menu_due(&self) -> bool {
        self.over && self.over_time >= GAME_OVER_DELAY_SECONDS
    }

    // -------------------------
    // Update
    // -------------------------
    /// Advance the rules by `dt` seconds. `spawn_points` are the world
    /// positions a ship may respawn at.
    pub fn update(&mut self, ships: &mut [Ship], spawn_points: &[(f64, f64)],
                  rng: &mut Rng, dt: f64) -> Vec<MatchEvent> {
        let mut events = Vec::new();
        if self.over {
            self.over_time += dt;
            return events;
        }

        for player in 0..ships.len() {
            self.players[player].go_timer = (self.players[player].go_timer - dt).max(0.0);

            match self.players[player].phase {
                Phase::Countdown => {
                    let p = &mut self.players[player];
                    p.timer -= dt;
                    if p.timer <= 0.0 {
                        p.phase = Phase::Alive;
                        p.go_timer = GO_MESSAGE_SECONDS;
                        ships[player].unfreeze(IMMUNITY_SECONDS);
                    }
                }
                Phase::Alive => {
                    if !ships[player].is_alive() {
                        let p = &mut self.players[player];
                        p.lives = p.lives.saturating_sub(1);
                        ships[player].kill();
                        events.push(MatchEvent::Died(player));
                        if p.lives == 0 {
                            p.phase = Phase::Out;
                        } else {
                            p.phase = Phase::Dead;
                            p.timer = DEATH_DELAY_SECONDS;
                        }
                    }
                }
                Phase::Dead => {
                    self.players[player].timer -= dt;
                    if self.players[player].timer <= 0.0 {
                        let (x, y) = pick_respawn(player, ships, spawn_points, rng);
                        ships[player].respawn(x, y);
                        self.begin_countdown(ships, player);
                        events.push(MatchEvent::Respawned(player));
                    }
                }
                Phase::Out => {}
            }
        }

        if self.players.iter().any(|p| p.phase == Phase::Out) {
            self.finish(ships, spawn_points, rng);
        }
        events
    }

    fn begin_countdown(&mut self, ships: &mut [Ship], player: usize) {
        let p = &mut self.players[player];
        p.phase = Phase::Countdown;
        p.timer = COUNTDOWN_SECONDS;
        p.go_timer = 0.0;
        ships[player].freeze();
    }

    /// Someone is out of lives. A survivor wins and is left flying,
    /// untouchable, while the result is shown.
    fn finish(&mut self, ships: &mut [Ship], spawn_points: &[(f64, f64)], rng: &mut Rng) {
        self.over = true;
        self.over_time = 0.0;
        let survivors: Vec<usize> = (0..self.players.len())
            .filter(|&i| self.players[i].phase != Phase::Out)
            .collect();
        if let [winner] = survivors[..] {
            self.winner = Some(winner);
            if ships[winner].dead {
                // was waiting to respawn
                let (x, y) = pick_respawn(winner, ships, spawn_points, rng);
                ships[winner].respawn(x, y);
            }
            let p = &mut self.players[winner];
            p.phase = Phase::Alive;
            p.go_timer = 0.0;
            ships[winner].unfreeze(f64::INFINITY);
        } else {
            self.winner = None;
        }
    }
}

/// Where a ship respawns: a random spawn point well away from the other
/// ships, or the furthest one if none is that far
pub fn pick_respawn(player: usize, ships: &[Ship], points: &[(f64, f64)],
                    rng: &mut Rng) -> (f64, f64) {
    if points.is_empty() {
        return (0.0, 0.0);
    }
    let clearance = |p: &(f64, f64)| {
        ships.iter().enumerate()
            .filter(|(i, _)| *i != player)
            .map(|(_, s)| (p.0 - s.x).hypot(p.1 - s.y))
            .fold(f64::INFINITY, f64::min)
    };
    let far: Vec<&(f64, f64)> = points.iter()
        .filter(|p| clearance(p) >= RESPAWN_MIN_DISTANCE)
        .collect();
    if !far.is_empty() {
        return *far[rng.below(far.len())];
    }
    let mut best = points[0];
    for p in points {
        if clearance(p) > clearance(&best) {
            best = *p;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f64 = 1.0 / 60.0;

    fn two_ships() -> Vec<Ship> {
        vec![Ship::new(100.0, 100.0), Ship::new(900.0, 900.0)]
    }

    fn step(m: &mut Match, ships: &mut [Ship], seconds: f64) -> Vec<MatchEvent> {
        let mut rng = Rng::new(1);
        let mut all = Vec::new();
        for _ in 0..(seconds / DT).round() as usize {
            all.extend(m.update(ships, &[(500.0, 500.0), (5000.0, 500.0)], &mut rng, DT));
            for s in ships.iter_mut() {
                s.update(DT, 0.0);
            }
        }
        all
    }

    #[test]
    fn the_match_starts_with_a_3_2_1_countdown_and_frozen_ships() {
        let mut ships = two_ships();
        let mut m = Match::new(&mut ships, 3);
        assert!(ships.iter().all(|s| s.frozen));
        let mut seen = Vec::new();
        for _ in 0..182 {
            step(&mut m, &mut ships, DT);
            if let Some(n) = m.countdown_number(0) {
                if seen.last() != Some(&n) {
                    seen.push(n);
                }
            }
        }
        assert_eq!(seen, vec![3, 2, 1]);
        assert!(!ships[0].frozen && ships[0].immune_time > 0.0 && m.show_go(0));
    }

    #[test]
    fn dying_costs_a_life_and_respawns_after_a_delay_with_a_countdown() {
        let mut ships = two_ships();
        let mut m = Match::new(&mut ships, 3);
        step(&mut m, &mut ships, 7.0); // countdown + immunity over
        ships[0].health.take_damage(500.0);
        let events = step(&mut m, &mut ships, DT);
        assert_eq!(events, vec![MatchEvent::Died(0)]);
        assert_eq!(m.lives(0), 2);
        assert!(ships[0].dead);
        let events = step(&mut m, &mut ships, 2.0);
        assert!(events.contains(&MatchEvent::Respawned(0)));
        assert!(ships[0].frozen && !ships[0].dead);
        assert_eq!(ships[0].health.hull, 100.0);
        assert_eq!(ships[0].x, 5000.0); // the spawn point far from the other ship
    }

    #[test]
    fn running_out_of_lives_ends_the_match_and_the_survivor_wins() {
        let mut ships = two_ships();
        let mut m = Match::new(&mut ships, 3);
        step(&mut m, &mut ships, 7.0);
        for _ in 0..3 {
            ships[0].frozen = false;
            ships[0].immune_time = 0.0;
            m.players[0].phase = Phase::Alive;
            ships[0].health.hull = 0.0;
            step(&mut m, &mut ships, 2.0);
        }
        assert!(m.over && m.winner == Some(1));
        assert_eq!(m.outcome(1), Some(Outcome::Victory));
        assert_eq!(m.outcome(0), Some(Outcome::Defeat));
        assert!(ships[1].immune_time.is_infinite() && !ships[1].frozen);
        assert!(!m.menu_due());
        step(&mut m, &mut ships, 4.0);
        assert!(m.menu_due());
    }

    #[test]
    fn a_winner_who_was_waiting_to_respawn_is_put_back_in_play() {
        let mut ships = two_ships();
        let mut m = Match::new(&mut ships, 3);
        for s in ships.iter_mut() { s.unfreeze(0.0); }
        for p in m.players.iter_mut() { p.phase = Phase::Alive; }
        let mut rng = Rng::new(1);
        ships[0].health.hull = 0.0;
        m.update(&mut ships, &[(500.0, 500.0)], &mut rng, 0.01);
        assert!(ships[0].dead);
        m.players[1].lives = 1;
        ships[1].health.hull = 0.0;
        m.update(&mut ships, &[(500.0, 500.0)], &mut rng, 0.01);
        assert_eq!(m.winner, Some(0));
        assert!(!ships[0].dead && !ships[0].frozen);
    }

    #[test]
    fn both_losing_their_last_life_together_is_a_draw() {
        let mut ships = two_ships();
        let mut m = Match::new(&mut ships, 1);
        for s in ships.iter_mut() { s.unfreeze(0.0); s.health.hull = 0.0; }
        for p in m.players.iter_mut() { p.phase = Phase::Alive; }
        m.update(&mut ships, &[], &mut Rng::new(1), 0.01);
        assert!(m.over && m.winner.is_none());
        assert_eq!(m.outcome(0), Some(Outcome::Draw));
    }

    #[test]
    fn respawn_avoids_the_other_ship_and_falls_back_to_the_furthest() {
        let ships = vec![Ship::new(0.0, 0.0), Ship::new(4900.0, 500.0)];
        let points = [(500.0, 500.0), (5000.0, 500.0), (100.0, 100.0)];
        let mut rng = Rng::new(7);
        for _ in 0..50 {
            let p = pick_respawn(0, &ships, &points, &mut rng);
            assert!(p == (500.0, 500.0) || p == (100.0, 100.0), "{p:?}");
        }
        // every point is within 1000 of the other ship: the furthest wins
        let near = [(4800.0, 500.0), (4600.0, 500.0)];
        assert_eq!(pick_respawn(0, &ships, &near, &mut rng), (4600.0, 500.0));
        assert_eq!(pick_respawn(0, &ships, &[], &mut rng), (0.0, 0.0));
    }
}
