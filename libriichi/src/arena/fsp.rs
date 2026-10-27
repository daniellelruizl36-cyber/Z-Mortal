//! FSP (fictitious self-play) arena: one challenger against a champion pool.
//!
//! Each game seats the challenger with opponents sampled from the pool:
//! `3 - n_older` latest champions plus `n_older` older ones (per the
//! 2x-latest + 1x-older league recipe; the promotion gate uses `n_older=0`,
//! i.e. 3x latest). Every engine gets one batched slot per game, so all
//! agent batches stay the same size. Seats are shuffled per game with a
//! deterministic RNG keyed by the game seed, so a given `(seed_start,
//! seed_count)` always reproduces the same seatings while different keys
//! decorrelate them.

use super::game::{BatchGame, Index};
use super::board::ReseedSpec;
use crate::agent::{BatchAgent, new_py_agent};
use crate::rule::RuleConfig;
use std::fs::{self, File};
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use flate2::Compression;
use flate2::read::GzEncoder;
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use pyo3::prelude::*;
use rayon::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Challenger,
    Latest,
    Older,
}

/// xorshift64* for reproducible seat shuffles.
struct SeatRng(u64);

impl SeatRng {
    fn new(seed: (u64, u64)) -> Self {
        let mut s = seed.0 ^ seed.1.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        if s == 0 {
            s = 0xD1B5_4A32_D192_ED03;
        }
        Self(s)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

#[pyclass]
#[derive(Clone, Default)]
pub struct FspArena {
    pub disable_progress_bar: bool,
    pub log_dir: Option<String>,
    pub rule: RuleConfig,
    /// VS1 A3 fork-based luck resimulation spec `(kyoku, after_kidx, nonce)`;
    /// None in all normal runs (zero behavior change).
    pub reseed: Option<(u8, u32, u64)>,
}

#[pymethods]
impl FspArena {
    #[new]
    #[pyo3(signature = (*, disable_progress_bar=false, log_dir=None, rule=None, reseed=None))]
    fn new(
        disable_progress_bar: bool,
        log_dir: Option<String>,
        rule: Option<String>,
        reseed: Option<(u8, u32, u64)>,
    ) -> Result<Self> {
        let rule = match rule {
            Some(s) if s == "zhuyi" => RuleConfig::zhuyi(),
            Some(s) if s == "zhuyi_guangzhou" => RuleConfig::zhuyi_guangzhou(),
            Some(s) => serde_json::from_str(&s).context("invalid rule config JSON")?,
            None => RuleConfig::default(),
        };
        Ok(Self {
            disable_progress_bar,
            log_dir,
            rule,
            reseed,
        })
    }

    /// Runs `seed_count` hanchans and returns one record per game:
    /// `(challenger_seat, final_scores, zhuyi_totals)`, with the arrays in
    /// seat order. CU synthesis happens on the Python side.
    #[pyo3(signature = (challenger, champ_latest, champ_older, seed_start, seed_count, n_older=1, seat_randomize=true))]
    pub fn py_vs_py(
        &self,
        challenger: PyObject,
        champ_latest: PyObject,
        champ_older: Option<PyObject>,
        seed_start: (u64, u64),
        seed_count: u64,
        n_older: u8,
        seat_randomize: bool,
        py: Python<'_>,
    ) -> Result<Vec<(u8, [i32; 4], [i32; 4])>> {
        ensure!(n_older <= 3, "n_older must be in 0..=3");
        let n_latest = 3usize - n_older as usize;
        if n_older > 0 && champ_older.is_none() {
            bail!("champ_older is required when n_older > 0");
        }
        // PyObject is not Send-shareable across allow_threads without
        // cloning the reference while the GIL is held.
        let latest_objs: Vec<PyObject> = (0..n_latest).map(|_| champ_latest.clone_ref(py)).collect();
        let older_objs: Vec<PyObject> = (0..n_older as usize)
            .map(|_| champ_older.as_ref().unwrap().clone_ref(py))
            .collect();

        py.allow_threads(move || {
            let (results, challenger_seats) = self.run_batch(
                challenger,
                latest_objs,
                older_objs,
                seed_start,
                seed_count,
                n_older,
                seat_randomize,
            )?;

            Ok(results
                .iter()
                .zip(challenger_seats)
                .map(|(r, seat)| (seat, r.scores, r.zhuyi_totals))
                .collect())
        })
    }
}

impl FspArena {
    /// Agent slot layout: [challenger, latest x n_latest, older x n_older].
    #[allow(clippy::too_many_arguments)]
    fn run_batch(
        &self,
        challenger: PyObject,
        latest_objs: Vec<PyObject>,
        older_objs: Vec<PyObject>,
        seed_start: (u64, u64),
        seed_count: u64,
        n_older: u8,
        seat_randomize: bool,
    ) -> Result<(Vec<super::result::GameResult>, Vec<u8>)> {
        ensure!(seed_count > 0, "seed_count must be positive");
        let n_older = n_older as usize;
        let n_latest = 3usize - n_older;
        debug_assert_eq!(latest_objs.len(), n_latest);
        debug_assert_eq!(older_objs.len(), n_older);

        if let Some(dir) = &self.log_dir {
            fs::create_dir_all(dir)?;
        }

        log::info!(
            "fsp seed: [{}, {}) w/ {:#x}, {} hanchans, {} latest + {} older",
            seed_start.0,
            seed_start.0 + seed_count,
            seed_start.1,
            seed_count,
            n_latest,
            n_older,
        );

        let seeds: Vec<(u64, u64)> = (seed_start.0..seed_start.0 + seed_count)
            .map(|seed| (seed, seed_start.1))
            .collect();

        // Seat every game deterministically, collecting per-instance player
        // ids and per-game agent indexes. Latest/Older instance indices
        // rotate by game index so instances stay balanced.
        let n_agents = 1 + n_latest + n_older;
        let mut player_ids: Vec<Vec<u8>> = vec![vec![]; n_agents];
        let mut pid_len = vec![0usize; n_agents];
        let mut indexes: Vec<[Index; 4]> = Vec::with_capacity(seeds.len());
        let mut challenger_seats: Vec<u8> = Vec::with_capacity(seeds.len());
        for (i, &seed) in seeds.iter().enumerate() {
            let mut kinds = vec![Kind::Challenger];
            kinds.extend(std::iter::repeat_n(Kind::Latest, n_latest));
            kinds.extend(std::iter::repeat_n(Kind::Older, n_older));
            let mut kinds: [Kind; 4] = kinds.try_into().expect("exactly 4 seats");
            if seat_randomize {
                let mut rng = SeatRng::new(seed);
                for j in (1..4).rev() {
                    let t = rng.below(j as u64 + 1) as usize;
                    kinds.swap(j, t);
                }
            }
            challenger_seats.push(
                kinds
                    .iter()
                    .position(|&k| k == Kind::Challenger)
                    .expect("one challenger") as u8,
            );

            let mut latest_seen = 0usize;
            let mut older_seen = 0usize;
            let mut idxs = [Index::default(); 4];
            for (seat, &kind) in kinds.iter().enumerate() {
                let agent_idx = match kind {
                    Kind::Challenger => 0,
                    Kind::Latest => {
                        let occ = latest_seen;
                        latest_seen += 1;
                        1 + (occ + i) % n_latest
                    }
                    Kind::Older => {
                        let occ = older_seen;
                        older_seen += 1;
                        1 + n_latest + (occ + i) % n_older
                    }
                };
                idxs[seat] = Index {
                    agent_idx,
                    player_id_idx: pid_len[agent_idx],
                };
                pid_len[agent_idx] += 1;
                player_ids[agent_idx].push(seat as u8);
            }
            indexes.push(idxs);
        }

        let mut agents: Vec<Box<dyn BatchAgent>> = Vec::with_capacity(n_agents);
        agents.push(new_py_agent(challenger, &player_ids[0])?);
        for (obj, ids) in latest_objs.into_iter().zip(&player_ids[1..1 + n_latest]) {
            agents.push(new_py_agent(obj, ids)?);
        }
        for (obj, ids) in older_objs.into_iter().zip(&player_ids[1 + n_latest..]) {
            agents.push(new_py_agent(obj, ids)?);
        }

        let batch_game = BatchGame::tenhou_hanchan(self.disable_progress_bar)
            .with_rule(self.rule)
            .with_reseed(self.reseed.map(|(kyoku, after_kidx, nonce)| ReseedSpec {
                kyoku,
                after_kidx,
                nonce,
            }));
        let results = batch_game.run(&mut agents, &indexes, &seeds)?;

        if let Some(dir) = &self.log_dir {
            log::info!("dumping game logs");

            let bar = if self.disable_progress_bar {
                ProgressBar::hidden()
            } else {
                ProgressBar::new(seed_count)
            };
            const TEMPLATE: &str = "[{elapsed_precise}] [{wide_bar}] {pos}/{len} {percent:>3}%";
            bar.set_style(ProgressStyle::with_template(TEMPLATE)?.progress_chars("#-"));
            bar.enable_steady_tick(Duration::from_millis(150));

            results
                .par_iter()
                .progress_with(bar)
                .enumerate()
                .try_for_each(|(_, game_result)| {
                    let (seed, key) = game_result.seed;
                    let filename: PathBuf =
                        [dir, &format!("{seed}_{key}_fsp.json.gz")].iter().collect();

                    let log = game_result.dump_json_log()?;
                    let mut comp = GzEncoder::new(log.as_bytes(), Compression::best());
                    let mut f = File::create(filename)?;
                    io::copy(&mut comp, &mut f)?;

                    anyhow::Ok(())
                })?;
        }

        Ok((results, challenger_seats))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn shuffled(seed: (u64, u64)) -> [usize; 4] {
        let mut rng = SeatRng::new(seed);
        let mut v = [0usize, 1, 2, 3];
        for j in (1..4).rev() {
            let t = rng.below(j as u64 + 1) as usize;
            v.swap(j, t);
        }
        v
    }

    #[test]
    fn seat_shuffle_is_reproducible() {
        let seed = (42u64, 0x2000u64);
        assert_eq!(shuffled(seed), shuffled(seed));
        // overwhelmingly unlikely to stay sorted for any key
        assert_ne!(shuffled(seed), [0, 1, 2, 3]);
        assert_ne!(shuffled((43, 0x2000)), [0, 1, 2, 3]);
    }

    #[test]
    fn seat_shuffle_covers_all_seats() {
        for seed in 0..64 {
            let v = shuffled((seed, 0x2000));
            let mut sorted = v;
            sorted.sort_unstable();
            assert_eq!(sorted, [0, 1, 2, 3]);
        }
    }
}
