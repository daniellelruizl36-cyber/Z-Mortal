//! Cross-check oracle for the zhuyi settlement: replays mjson logs through
//! `PlayerState` and emits the zhuyi outcome of every hora as JSONL, to be
//! diffed against `zhuyi/settlement.py` (the user-confirmed oracle).
//!
//! Usage: zhuyi_replay <FILE.mjson | DIR>...

use riichi::mjai::Event;
use riichi::state::PlayerState;
use riichi::{RuleConfig, tu8};
use riichi::zhuyi;

use std::env;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use glob::glob;
use serde_json as json;

/// Pao tracking with the same semantics as `settlement.py::_pending_pao`:
/// the supplier of the 3rd dragon koutsu (pon/daiminkan), the 4th wind
/// koutsu, or the 4th kan (when it is a daiminkan) of a player. Multiple
/// entries per player are kept as `(supplier, covered units)` shares so
/// compound yakuman settle per unit.
#[derive(Default)]
struct PaoTracker {
    /// player -> dragon koutsu count, wind koutsu count, kan count
    dragons: [u8; 4],
    winds: [u8; 4],
    kans: [u8; 4],
    /// player -> pao shares (supplier, covered units), merged per supplier
    pending: [Vec<(u8, u8)>; 4],
}

impl PaoTracker {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn add(&mut self, actor: u8, supplier: u8) {
        let list = &mut self.pending[actor as usize];
        if let Some(entry) = list.iter_mut().find(|(s, _)| *s == supplier) {
            entry.1 += 1;
        } else {
            list.push((supplier, 1));
        }
    }

    fn on_call(&mut self, actor: u8, target: u8, koutsu_tile: riichi::tile::Tile) {
        let tile = koutsu_tile.as_u8();
        let is_dragon = (tu8!(P)..=tu8!(C)).contains(&tile);
        let is_wind = (tu8!(E)..tu8!(P)).contains(&tile);
        if is_dragon {
            self.dragons[actor as usize] += 1;
            if self.dragons[actor as usize] == 3 {
                self.add(actor, target);
            }
        } else if is_wind {
            self.winds[actor as usize] += 1;
            if self.winds[actor as usize] == 4 {
                self.add(actor, target);
            }
        }
    }

    fn on_daiminkan(&mut self, actor: u8, target: u8) {
        self.kans[actor as usize] += 1;
        if self.kans[actor as usize] == 4 {
            self.add(actor, target);
        }
    }

    fn on_ankan_or_kakan(&mut self, actor: u8) {
        self.kans[actor as usize] += 1;
    }
}

fn process_path(path: &Path, out: &mut impl Write) -> Result<()> {
    // Local dataset files are gzip-compressed despite the `.mjson`
    // extension, so sniff the magic bytes instead of trusting the name.
    let raw = std::fs::read(path)?;
    let raw_log = if raw.starts_with(&[0x1f, 0x8b]) {
        io::read_to_string(GzDecoder::new(&raw[..]))?
    } else {
        String::from_utf8(raw)?
    };
    let events: Vec<Event> = raw_log
        .lines()
        .map(|l| Ok(json::from_str(l)?))
        .collect::<Result<_>>()?;

    let mut states = [
        PlayerState::new(0),
        PlayerState::new(1),
        PlayerState::new(2),
        PlayerState::new(3),
    ];
    // `settlement.py` semantics: no kazoe yakuman, luck yakuman stacks with
    // pattern yakuman (units = number of structural yakuman patterns).
    let rule = RuleConfig::zhuyi();
    let mut pao = PaoTracker::default();
    // Counts start_kyoku like `settlement.py::_kyoku_idx` (0-based); a drawn
    // round is logged as `ryukyoku` instead of `end_kyoku`, so counting
    // ends would drift.
    let mut kyokus_started = 0_u8;
    let mut kyoku = 0_u8;

    for ev in &events {
        match ev {
            Event::StartKyoku { .. } => {
                pao.reset();
                kyoku = kyokus_started;
                kyokus_started += 1;
            }
            Event::StartGame { .. } => {
                kyokus_started = 0;
                kyoku = 0;
            }
            Event::Pon {
                actor, target, pai, ..
            } => {
                pao.on_call(*actor, *target, *pai);
            }
            Event::Daiminkan {
                actor, target, pai, ..
            } => {
                pao.on_call(*actor, *target, *pai);
                pao.on_daiminkan(*actor, *target);
            }
            Event::Ankan { actor, .. } => {
                pao.on_ankan_or_kakan(*actor);
            }
            Event::Kakan { actor, .. } => {
                pao.on_ankan_or_kakan(*actor);
            }
            Event::Hora {
                actor,
                target,
                ura_markers,
                ..
            } => {
                let is_ron = actor != target;
                let ura_markers = ura_markers.clone().unwrap_or_default();
                let state = &states[*actor as usize];
                let (_point, agari) = state
                    .agari_point_and_kind(is_ron, &ura_markers, &rule)
                    .with_context(|| {
                        format!(
                            "failed to get agari at kyoku {kyoku}\naction: {ev:?}\nstate:\n{}",
                            state.brief_info()
                        )
                    })?;
                let yakuman_units = match agari {
                    riichi::algo::agari::Agari::Yakuman(n) => n,
                    riichi::algo::agari::Agari::Normal { .. } => 0,
                };
                let counts = state.zhuyi_counts(&ura_markers, is_ron);
                if std::env::var_os("ZHUYI_DEBUG").is_some() {
                    eprintln!(
                        "DEBUG kyoku={kyoku} actor={actor} markers={ura_markers:?} counts={counts:?}\n{}\n",
                        state.brief_info()
                    );
                }
                let pao_shares: &[(u8, u8)] = if yakuman_units > 0 {
                    &pao.pending[*actor as usize]
                } else {
                    &[]
                };
                let deltas = zhuyi::settle(*actor, *target, counts, yakuman_units, pao_shares);

                let record = json::json!({
                    "file": path.display().to_string(),
                    "kyoku": kyoku,
                    "actor": actor,
                    "target": target,
                    "is_ron": is_ron,
                    "counts": counts,
                    "yakuman_units": yakuman_units,
                    "pao_shares": pao_shares,
                    "deltas": deltas,
                });
                writeln!(out, "{record}")?;
            }
            _ => (),
        }

        for state in &mut states {
            // `validate_logs` pattern: keep the action candidates across
            // announce events so that multi-ron horas after the first can
            // still be evaluated against the state of the triggering discard.
            state.update_with_keep_cans(ev, true)?;
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.is_empty() {
        bail!("Usage: zhuyi_replay <FILE.mjson | DIR>...");
    }

    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());

    for arg in &args {
        let path = Path::new(arg);
        if path.is_file() {
            process_path(path, &mut out)?;
            continue;
        }
        for entry in glob(&format!("{arg}/**/*.json"))?.chain(glob(&format!("{arg}/**/*.json.gz"))?)
        {
            process_path(&entry?, &mut out)?;
        }
    }

    out.flush()?;
    Ok(())
}
