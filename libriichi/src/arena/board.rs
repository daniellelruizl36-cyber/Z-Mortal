use super::result::KyokuResult;
use crate::array::Simple2DArray;
use crate::consts::oracle_obs_shape;
use crate::mjai::{Event, EventExt, Metadata};
use crate::rule::RuleConfig;
use crate::state::PlayerState;
use crate::tile::Tile;
use crate::vec_ops::vec_add_assign;
use crate::{matches_tu8, must_tile, t, tu8};
use std::convert::TryInto;
use std::{array, mem};

use anyhow::{Context, Result, bail};
use derivative::Derivative;
use ndarray::prelude::*;
use rand::prelude::*;
use rand_chacha::ChaCha12Rng;
use sha3::{Digest, Sha3_256};
use tinyvec::ArrayVec;

/// Optional wall reshuffle spec for fork-based luck resimulation (VS1 A3).
///
/// When set, the board watches its per-kyoku filtered event count (same
/// FILTER convention as the Python fork director: start_game/end_game/dora
/// excluded). Right after the `after_kidx`-th filtered event is applied, the
/// remaining `yama` (draw supply) is reshuffled with a nonce-seeded RNG.
/// Haipai, rinshan, dora and ura piles are untouched, so everything up to and
/// including that event stays bit-identical to the original game — only the
/// post-event draw order changes. `None` = zero behavior change.
#[derive(Debug, Clone, Copy)]
pub struct ReseedSpec {
    /// Apply only in this kyoku (0-based sequential start_kyoku index,
    /// counting renchan — the fp_fork7 `kyoku_id` convention).
    pub kyoku: u8,
    /// Filtered index of the trigger event; reshuffle fires right after it.
    pub after_kidx: u32,
    /// Resimulation nonce; different nonces = independent reshuffles.
    pub nonce: u64,
}

/// The fields are all pub on purpose so the caller will be able to set the
/// yama, doras, scores directly.
///
/// Other than what is mentioned below, everything else is identical to Tenhou's
/// Rule.
///
/// 1. No triple-ron ryukyoku.
/// 2. Tenhou (the yaku) and chihou do not accumulate with other yakus; they are
///    always 1x yakuman.
#[derive(Debug, Default)]
pub struct Board {
    /// Counts from 0
    pub kyoku: u8,
    pub honba: u8,
    /// Does not effect the kyoku seed
    pub kyotaku: u8,
    /// [25000; 4]
    pub scores: [i32; 4],

    pub haipai: [[Tile; 13]; 4],
    /// Goes backward (pop)
    pub yama: Vec<Tile>,
    /// Goes backward (pop)
    pub rinshan: Vec<Tile>,
    /// Goes backward (pop)
    pub dora_indicators: Vec<Tile>,
    /// Goes forward (iter)
    pub ura_indicators: Vec<Tile>,

    /// Custom rule switches; defaults to Tenhou's rule.
    pub rule: RuleConfig,

    /// VS1 A3 fork-based luck resimulation; see `ReseedSpec`.
    pub reseed: Option<ReseedSpec>,
}

#[derive(Derivative)]
#[derivative(Default)]
pub struct BoardState {
    board: Board,
    // Absolute seat, with the oya of E1 always being 0
    oya: u8,
    player_states: [PlayerState; 4],
    rule: RuleConfig,

    can_renchan: bool,
    has_hora: bool,
    has_abortive_ryukyoku: bool,
    kyoku_deltas: [i32; 4],
    /// Zhuyi sheets won/lost this kyoku (`rule.zhuyi`).
    kyoku_zhuyi: [i32; 4],

    #[derivative(Default(value = "70"))]
    tiles_left: u8,
    tsumo_actor: u8,
    // Just a fancy bool
    deal_from_rinshan: Option<()>,
    need_new_dora_at_discard: Option<()>,
    need_new_dora_at_tsumo: Option<()>,
    riichi_to_be_accepted: Option<u8>,
    #[derivative(Default(value = "[true; 4]"))]
    can_nagashi_mangan: [bool; 4],
    #[derivative(Default(value = "true"))]
    can_four_wind: bool,
    four_wind_tile: Option<Tile>,
    accepted_riichis: u8,
    kans: u8,
    check_four_kan: bool,
    /// Pao suppliers per player: `(supplier, covered yakuman units)`. A
    /// player can have up to two entries (e.g. a dragon/wind pao plus a
    /// suukantsu pao); covered units are settled separately per entry.
    paos: [Vec<(u8, u8)>; 4],
    /// Set while waiting for the voluntary noten declarations at an
    /// exhaustive draw (`rule.voluntary_noten`).
    noten_phase: bool,

    log: Vec<EventExt>,

    /// VS1 A3: reshuffle spec for this kyoku (already filtered by the
    /// sequential kyoku index in `Game::poll`); None in all normal runs.
    reseed: Option<ReseedSpec>,
    /// VS1 A3: filtered events pushed so far this kyoku.
    filtered_pushed: u32,
    /// VS1 A3: whether the reshuffle already fired.
    reseed_done: bool,

    // For oracle_obs only
    dora_indicators_full: Vec<Tile>,
}

pub struct AgentContext<'a> {
    pub player_states: &'a [PlayerState; 4],
    pub log: &'a [EventExt],
}

#[derive(Clone, Copy)]
pub enum Poll {
    InGame,
    End,
}

impl Board {
    pub fn init_from_seed(&mut self, game_seed: (u64, u64)) {
        let (nonce, key) = game_seed;
        let kyoku_seed = Sha3_256::new()
            .chain_update(nonce.to_le_bytes())
            .chain_update(key.to_le_bytes())
            .chain_update([self.kyoku, self.honba])
            .finalize()
            .into();
        let mut rng = ChaCha12Rng::from_seed(kyoku_seed);
        let mut seq = UNSHUFFLED;
        seq.shuffle(&mut rng);

        self.haipai = array::from_fn(|i| seq[i * 13..(i + 1) * 13].try_into().unwrap());
        let mut idx = 13 * 4;

        self.rinshan = seq[idx..idx + 4].to_vec();
        idx += 4;
        self.dora_indicators = seq[idx..idx + 5].to_vec();
        idx += 5;
        self.ura_indicators = seq[idx..idx + 5].to_vec();
        idx += 5;
        self.yama = seq[idx..idx + 70].to_vec();
        idx += 70;
        assert_eq!(idx, seq.len());
    }

    pub fn into_state(self) -> BoardState {
        let oya = self.kyoku % 4;
        let dora_indicators_full = self.dora_indicators.clone();
        let rule = self.rule;
        // Kyoku matching happens in Game::poll against the sequential
        // start_kyoku index (fp_fork7 kyoku_id convention, counts renchan).
        let reseed = self.reseed;

        BoardState {
            board: self,
            oya,
            player_states: array::from_fn(|i| {
                let mut s = PlayerState::new(i as u8);
                s.kyuushu_kyuuhai = rule.kyuushu_kyuuhai;
                s
            }),
            dora_indicators_full,
            rule,
            reseed,
            ..Default::default()
        }
    }
}

impl BoardState {
    /// Returns iff any player on the board can act or the kyoku has ended.
    pub fn poll(&mut self, mut reactions: [EventExt; 4]) -> Result<Poll> {
        loop {
            let poll = self.step(&reactions)?;
            match poll {
                Poll::InGame => {
                    if self.player_states.iter().any(|c| c.last_cans().can_act()) {
                        return Ok(poll);
                    }
                }
                Poll::End => {
                    self.add_log_no_meta(Event::EndKyoku);
                    vec_add_assign(&mut self.board.scores, &self.kyoku_deltas);
                    if self.has_abortive_ryukyoku {
                        self.can_renchan = true;
                    }
                    return Ok(poll);
                }
            };
            reactions = Default::default();
        }
    }

    #[inline]
    pub fn agent_context(&self) -> AgentContext<'_> {
        AgentContext {
            player_states: &self.player_states,
            log: &self.log,
        }
    }

    #[inline]
    pub const fn end(&self) -> KyokuResult {
        KyokuResult {
            kyoku: self.board.kyoku,
            // honba: self.board.honba,
            can_renchan: self.can_renchan,
            has_hora: self.has_hora,
            has_abortive_ryukyoku: self.has_abortive_ryukyoku,
            kyotaku_left: self.board.kyotaku,
            scores: self.board.scores,
            zhuyi_deltas: self.kyoku_zhuyi,
        }
    }

    #[inline]
    pub fn take_log(&mut self) -> Vec<EventExt> {
        mem::take(&mut self.log)
    }

    #[inline]
    fn add_log(&mut self, ev: EventExt) {
        self.note_pushed(&ev.event);
        self.log.push(ev);
    }

    #[inline]
    fn add_log_no_meta(&mut self, ev: Event) {
        self.note_pushed(&ev);
        self.log.push(EventExt::no_meta(ev));
    }

    #[inline]
    fn add_log_zhuyi(&mut self, ev: Event, zhuyi: Option<Metadata>) {
        self.note_pushed(&ev);
        self.log.push(EventExt {
            event: ev,
            meta: zhuyi,
        });
    }

    /// VS1 A3: track the filtered event count and fire the yama reshuffle
    /// right after the trigger event lands (see `ReseedSpec`). Must be called
    /// before pushing the event to `log`.
    fn note_pushed(&mut self, ev: &Event) {
        if matches!(ev, Event::Dora { .. }) {
            return;
        }
        self.filtered_pushed += 1;
        if self.reseed_done {
            return;
        }
        if let Some(spec) = self.reseed {
            if self.filtered_pushed == spec.after_kidx + 1 {
                let mut hasher = Sha3_256::new();
                hasher.update(b"zmortal-wall-reseed-v1");
                hasher.update(spec.nonce.to_le_bytes());
                hasher.update([self.board.kyoku, self.board.honba]);
                hasher.update(self.filtered_pushed.to_le_bytes());
                let seed: [u8; 32] = hasher.finalize().into();
                let mut rng = ChaCha12Rng::from_seed(seed);
                self.board.yama.shuffle(&mut rng);
                self.reseed_done = true;
            }
        }
    }

    #[inline]
    fn broadcast(&mut self, ev: &Event) {
        for s in &mut self.player_states {
            s.update(ev).expect("fatal internal bug in BoardState");
        }
    }

    fn haipai(&mut self) -> Result<()> {
        let bakaze = must_tile!(tu8!(E) + self.board.kyoku / 4);
        let start_kyoku = Event::StartKyoku {
            bakaze,
            dora_marker: self
                .board
                .dora_indicators
                .pop()
                .context("insufficient dora indicators")?,
            kyoku: self.oya + 1,
            honba: self.board.honba,
            kyotaku: self.board.kyotaku,
            oya: self.oya,
            scores: self.board.scores,
            tehais: self.board.haipai,
        };
        self.broadcast(&start_kyoku);
        self.add_log_no_meta(start_kyoku);

        let tile = self
            .board
            .yama
            .pop()
            .context("invalid yama: empty at init")?;
        self.tiles_left -= 1;
        let first_tsumo = Event::Tsumo {
            actor: self.oya,
            pai: tile,
        };
        self.broadcast(&first_tsumo);
        self.add_log_no_meta(first_tsumo);

        Ok(())
    }

    /// `noten_declared[i] == true` forces player `i` to be treated as noten,
    /// regardless of the automatic shanten detection
    /// (`rule.voluntary_noten`).
    fn exhaustive_ryukyoku(&mut self, noten_declared: Option<&[bool; 4]>) {
        let mut deltas = [0; 4];
        let is_tenpai = |state: &PlayerState, declared: bool| -> bool {
            state.shanten() == 0 && !declared
        };
        self.can_renchan = is_tenpai(
            &self.player_states[self.oya as usize],
            noten_declared.is_some_and(|d| d[self.oya as usize]),
        );

        let mut has_nagashi_mangan = false;
        if self.rule.nagashi_mangan {
            self.can_nagashi_mangan
                .iter()
                .enumerate()
                .filter(|&(_, &b)| b)
                .map(|(i, _)| i)
                .for_each(|i| {
                    has_nagashi_mangan = true;
                    if i as u8 == self.oya {
                        let mut dod = [-4000; 4];
                        dod[i] = 12000;
                        vec_add_assign(&mut deltas, &dod);
                    } else {
                        let mut dod = [-2000; 4];
                        dod[i] = 8000;
                        dod[self.oya as usize] = -4000;
                        vec_add_assign(&mut deltas, &dod);
                    };
                });
        }

        if !has_nagashi_mangan {
            let tenpai_actors: ArrayVec<[_; 4]> = self
                .player_states
                .iter()
                .enumerate()
                .filter(|&(i, s)| is_tenpai(s, noten_declared.is_some_and(|d| d[i])))
                .map(|(i, _)| i)
                .collect();

            let (plus, minus) = match tenpai_actors.len() {
                1 => (3000, -1000),
                2 => (1500, -1500),
                3 => (1000, -3000),
                // 0 | 4
                _ => (0, 0),
            };
            if plus > 0 {
                let mut dod = [minus; 4];
                tenpai_actors.into_iter().for_each(|i| dod[i] = plus);
                vec_add_assign(&mut deltas, &dod);
            }
        }

        vec_add_assign(&mut self.kyoku_deltas, &deltas);
        let ryukyoku = Event::Ryukyoku {
            deltas: Some(deltas),
        };
        self.add_log_no_meta(ryukyoku);
        // no need to broadcast
    }

    const fn update_nagashi_mangan_and_four_wind(&mut self, ev: &Event) {
        match *ev {
            Event::Dahai { actor, pai, .. } if !pai.is_yaokyuu() => {
                self.can_nagashi_mangan[actor as usize] = false;
            }
            Event::Chi { target, .. }
            | Event::Pon { target, .. }
            | Event::Daiminkan { target, .. } => {
                self.can_nagashi_mangan[target as usize] = false;
                self.can_four_wind = false;
            }
            Event::Ankan { .. } => {
                self.can_four_wind = false;
            }
            _ => (),
        };
    }

    fn check_four_wind(&mut self, pai: Tile) -> Result<bool> {
        if !matches_tu8!(pai.as_u8(), E | S | W | N) {
            self.can_four_wind = false;
        } else if self.player_states[self.tsumo_actor as usize].can_w_riichi() {
            if let Some(tile) = self.four_wind_tile {
                // compare if the tile is equal to the first
                // wind
                self.can_four_wind = tile == pai;
            } else {
                // the very first discard and it is a wind,
                // record the wind
                self.four_wind_tile = Some(pai);
            }
        } else if let Some(tile) = self.four_wind_tile {
            // check if the first jun is just over and the last
            // discarded wind is still the same as the previous
            if tile == pai {
                return Ok(true);
            }
            // do not bother checking it again
            self.can_four_wind = false;
        } else {
            bail!("unexpected state when calculating 四風連打");
        }

        Ok(false)
    }

    fn check_riichi_accepted(&mut self) {
        if let Some(actor) = self.riichi_to_be_accepted.take() {
            let riichi_accepted = Event::ReachAccepted { actor };
            self.broadcast(&riichi_accepted);
            self.add_log_no_meta(riichi_accepted);
            self.board.scores[actor as usize] -= 1000;
            self.board.kyotaku += 1;
            self.accepted_riichis += 1;
        }
    }

    fn add_new_dora(&mut self) -> Result<()> {
        let dora = self
            .board
            .dora_indicators
            .pop()
            .context("illegal kan: already 4 kans and this is the 5th")?;
        let dora_ev = Event::Dora { dora_marker: dora };
        self.broadcast(&dora_ev);
        self.add_log_no_meta(dora_ev);

        Ok(())
    }

    fn handle_hora(
        &mut self,
        single_actor: u8,
        single_target: u8,
        reactions: &[EventExt; 4],
    ) -> Result<()> {
        self.has_hora = true;

        let is_ron = single_actor != single_target;
        let mut honba_left = self.board.honba as i32; // mut in case of multi-ron
        let mut kyotaku_point = self.board.kyotaku as i32 * 1000; // ditto
        self.board.kyotaku = 0; // Unlike honba, kyotaku in self will be cleared

        // Let the states get their agari points provided with our ura
        // indicators.
        let ura_indicators =
            self.board.ura_indicators[..5 - self.board.dora_indicators.len()].to_vec();
        let points = reactions
            .iter()
            .map(|ev| match ev.event {
                Event::Hora { actor, .. } => {
                    self.can_renchan |= actor == self.oya;
                    let agari = self.player_states[actor as usize].agari_point_and_kind(
                        is_ron,
                        &ura_indicators,
                        &self.rule,
                    );
                    Some(agari).transpose()
                }
                _ => Ok(None),
            })
            .collect::<Result<Vec<_>>>()?;

        if is_ron {
            // Multi-ron will be handled
            points
                .into_iter()
                .enumerate()
                .cycle()
                .skip(single_target as usize + 1)
                .take(3)
                .filter_map(|(actor, v)| v.map(|agari| (actor, agari)))
                .for_each(|(actor, (point, agari))| {
                    let mut deltas = [0; 4];
                    let units = match agari {
                        crate::algo::agari::Agari::Yakuman(n) => i64::from(n),
                        crate::algo::agari::Agari::Normal { .. } => 0,
                    };
                    let pao_shares = self.paos[actor].clone();
                    if units > 0 && !pao_shares.is_empty() {
                        // Compound yakuman with pao: each supplier covers
                        // only their units — supplier pays `ron * k / (2n)`,
                        // the discarder pays the rest of the ron (uncovered
                        // units in full + half of the covered part).
                        // 積み棒は包: honba goes to the first supplier.
                        // As per [Tenhou's rule](https://tenhou.net/man/#RULE):
                        //
                        // > 複合役満を含む得点を、ツモ＝全額・ロン＝折半で支払
                        // > う。積み棒は包。
                        let ron = i64::from(point.ron);
                        let denom = 2 * units;
                        let mut covered = 0_i64;
                        for &(sup, k) in &pao_shares {
                            let k = i64::from(k).clamp(0, units - covered);
                            if k == 0 {
                                continue;
                            }
                            covered += k;
                            deltas[sup as usize] -= (ron * k / denom) as i32;
                        }
                        deltas[single_target as usize] -=
                            (ron - ron * covered / denom) as i32;
                        deltas[pao_shares[0].0 as usize] -= honba_left * 300;
                    } else {
                        deltas[single_target as usize] = -point.ron - honba_left * 300;
                    }
                    deltas[actor] = point.ron + kyotaku_point + honba_left * 300;

                    kyotaku_point = 0;
                    honba_left = 0;

                    vec_add_assign(&mut self.kyoku_deltas, &deltas);
                    let ura_markers = if self.player_states[actor].self_riichi_accepted() {
                        ura_indicators.clone()
                    } else {
                        Default::default()
                    };

                    let zhuyi = self.zhuyi_outcome(actor as u8, single_target, true, agari);
                    let hora = Event::Hora {
                        actor: actor as u8,
                        target: single_target,
                        deltas: Some(deltas),
                        ura_markers: Some(ura_markers),
                    };
                    self.add_log_zhuyi(hora, zhuyi);
                    // No need to broadcast
                });
            return Ok(());
        }

        let (point, agari) = points[single_actor as usize].unwrap();
        let mut deltas = [0; 4];
        let units = match agari {
            crate::algo::agari::Agari::Yakuman(n) => i64::from(n),
            crate::algo::agari::Agari::Normal { .. } => 0,
        };
        let pao_shares = &self.paos[single_actor as usize];
        if units > 0 && !pao_shares.is_empty() {
            // Per-unit pao: each supplier pays `ron * k / n` for their
            // covered units; every opponent (suppliers included) pays their
            // normal tsumo share scaled by the uncovered fraction
            // `(n - k) / n`. With full cover this reduces to the classic
            // ツモ＝全額: the supplier pays the whole ron and the others pay
            // nothing. 積み棒は包.
            let ron = i64::from(point.ron);
            let mut covered = 0_i64;
            for &(_, k) in pao_shares {
                covered += i64::from(k).clamp(0, units - covered);
            }
            let uncovered = units - covered;
            for p in 0..4_usize {
                if p == single_actor as usize {
                    continue;
                }
                let share = if p == self.oya as usize {
                    point.tsumo_oya
                } else {
                    point.tsumo_ko
                };
                deltas[p] -= (i64::from(share) * uncovered / units) as i32;
            }
            for &(sup, k) in pao_shares {
                let k = i64::from(k).clamp(0, units);
                deltas[sup as usize] -= (ron * k / units) as i32;
            }
            deltas[pao_shares[0].0 as usize] -= honba_left * 300;
        } else {
            deltas.fill(-point.tsumo_ko - honba_left * 100);
            if single_actor != self.oya {
                deltas[self.oya as usize] = -point.tsumo_oya - honba_left * 100;
            }
        };
        deltas[single_actor as usize] =
            point.tsumo_total(single_actor == self.oya) + kyotaku_point + honba_left * 300;

        vec_add_assign(&mut self.kyoku_deltas, &deltas);
        let ura_markers = if self.player_states[single_actor as usize].self_riichi_accepted() {
            ura_indicators
        } else {
            Default::default()
        };

        let hora = Event::Hora {
            actor: single_actor,
            target: single_target,
            deltas: Some(deltas),
            ura_markers: Some(ura_markers),
        };
        let zhuyi = self.zhuyi_outcome(single_actor, single_target, false, agari);
        self.add_log_zhuyi(hora, zhuyi);
        // No need to broadcast

        Ok(())
    }

    /// Compute the zhuyi outcome for a hora and accumulate the kyoku sheets.
    /// Returns `None` when `rule.zhuyi` is off.
    fn zhuyi_outcome(
        &mut self,
        actor: u8,
        target: u8,
        is_ron: bool,
        agari: crate::algo::agari::Agari,
    ) -> Option<crate::mjai::Metadata> {
        if !self.rule.zhuyi {
            return None;
        }
        let ura_markers = if self.player_states[actor as usize].self_riichi_accepted() {
            self.board.ura_indicators[..5 - self.board.dora_indicators.len()].to_vec()
        } else {
            Vec::new()
        };
        let mut counts = self.player_states[actor as usize].zhuyi_counts(&ura_markers, is_ron);
        // Guangzhou rule: an open hand forfeits the aka sheets entirely.
        if self.rule.zhuyi_menzen_only && !self.player_states[actor as usize].is_menzen() {
            counts.aka = 0;
        }
        let yakuman_units = match agari {
            crate::algo::agari::Agari::Yakuman(n) => n,
            crate::algo::agari::Agari::Normal { .. } => 0,
        };
        let deltas = super::zhuyi::settle(actor, target, counts, yakuman_units, &self.paos[actor as usize]);
        vec_add_assign(&mut self.kyoku_zhuyi, &deltas);
        Some(crate::mjai::Metadata {
            zhuyi: Some(super::zhuyi::ZhuyiOutcome {
                counts,
                yakuman_units,
                deltas,
            }),
            ..Default::default()
        })
    }

    /// Record a pao supplier for `actor`, merging into an existing entry
    /// for the same supplier (one supplier can be responsible for two units,
    /// e.g. both the dragon koutsu and the 4th kan of a compound yakuman).
    fn add_pao(&mut self, actor: u8, supplier: u8) {
        let list = &mut self.paos[actor as usize];
        if let Some(entry) = list.iter_mut().find(|(s, _)| *s == supplier) {
            entry.1 += 1;
        } else {
            list.push((supplier, 1));
        }
    }

    fn update_paos(&mut self, ev: &Event) {
        // 四槓子の包（R8）：供出第 4 副杠的大明杠者包牌。不提前返回——这张
        // 大明杠若是三元/风牌刻子，同时触发对应的役满包牌（复合役满各记一份）。
        if self.rule.suukantsu_pao
            && let Event::Daiminkan { target, actor, .. } = *ev
            && self.player_states[actor as usize].kans_count() == 4
        {
            self.add_pao(actor, target);
        }

        match *ev {
            Event::Pon {
                target, actor, pai, ..
            }
            | Event::Daiminkan {
                target, actor, pai, ..
            } if pai.is_jihai() => {
                let mut jihais = 0_u8;
                self.player_states[actor as usize]
                    .pons()
                    .iter()
                    .chain(self.player_states[actor as usize].minkans())
                    .copied()
                    .filter(|&t| t >= tu8!(E))
                    .for_each(|t| jihais |= 1 << (t - tu8!(E)));
                let daisanein_confirmed = (jihais & 0b1110000) == 0b1110000;
                let daisuushi_confirmed = (jihais & 0b0001111) == 0b0001111;
                if daisanein_confirmed && matches_tu8!(pai.as_u8(), P | F | C)
                    || daisuushi_confirmed && matches_tu8!(pai.as_u8(), E | S | W | N)
                {
                    self.add_pao(actor, target);
                }
            }
            _ => (),
        }
    }

    #[inline]
    fn abortive_ryukyoku(&mut self) {
        let ryukyoku = Event::Ryukyoku {
            deltas: Some([0; 4]),
        };
        self.add_log_no_meta(ryukyoku);
        self.has_abortive_ryukyoku = true;
        // No need to broadcast
    }

    fn step(&mut self, reactions: &[EventExt; 4]) -> Result<Poll> {
        if self.tiles_left == 70 {
            self.haipai()?;
            return Ok(Poll::InGame);
        }

        if self.rule.suucha_riichi && self.accepted_riichis == 4 {
            // 四家立直
            self.abortive_ryukyoku();
            return Ok(Poll::End);
        }

        // Validate reactions
        for (actor, ev) in reactions.iter().enumerate() {
            self.player_states[actor]
                .validate_reaction(&ev.event)
                .with_context(|| {
                    format!(
                        "invalid action: {ev:?}\nstate:\n{}",
                        self.player_states[actor].brief_info(),
                    )
                })?;
        }

        if self.noten_phase {
            // Reactions to the voluntary noten declaration round are in;
            // players who sent Ryukyoku declared noten.
            let mut declared = [false; 4];
            for (actor, ev) in reactions.iter().enumerate() {
                declared[actor] = matches!(ev.event, Event::Ryukyoku { .. });
            }
            self.noten_phase = false;
            self.exhaustive_ryukyoku(Some(&declared));
            return Ok(Poll::End);
        }

        let ev = reactions
            .iter()
            .min_by_key(|ev| match ev.event {
                Event::Hora { .. } => 0,
                Event::Daiminkan { .. } | Event::Pon { .. } => 1,
                Event::None => 3,
                _ => 2,
            })
            .unwrap(); // Unwrap is safe because it is proven non-empty

        if self.rule.suukantsu_draw && self.check_four_kan && !matches!(ev.event, Event::Hora { .. })
        {
            // 四槓散了
            self.abortive_ryukyoku();
            return Ok(Poll::End);
        }

        self.update_nagashi_mangan_and_four_wind(&ev.event);

        match ev.event {
            Event::None => {
                if self.tiles_left == 0 {
                    if self.rule.voluntary_noten && !self.noten_phase {
                        // Open the declaration round: every player may send
                        // Ryukyoku to declare noten, or None to accept the
                        // automatic tenpai detection.
                        self.noten_phase = true;
                        for s in &mut self.player_states {
                            s.open_noten_declare();
                        }
                        return Ok(Poll::InGame);
                    }
                    self.exhaustive_ryukyoku(None);
                    return Ok(Poll::End);
                }
                self.check_riichi_accepted();

                let tile = if self.deal_from_rinshan.take().is_some() {
                    self.board
                        .rinshan
                        .pop()
                        .context("illegal kan: already 4 kans and this is the 5th")?
                } else {
                    self.board.yama.pop().with_context(|| {
                        format!("tiles left > 0 ({}) but yama is empty", self.tiles_left)
                    })?
                };
                self.tiles_left -= 1;
                let tsumo = Event::Tsumo {
                    actor: self.tsumo_actor,
                    pai: tile,
                };

                // This is for Kakan only because chankan is possible until an
                // actual tsumo.
                if self.need_new_dora_at_tsumo.take().is_some() {
                    self.add_new_dora()?;
                }

                self.broadcast(&tsumo);
                self.add_log_no_meta(tsumo);
            }

            Event::Dahai { actor, pai, .. } => {
                if self.need_new_dora_at_discard.take().is_some() {
                    self.add_new_dora()?;
                }

                self.broadcast(&ev.event);
                self.add_log(ev.clone());
                self.tsumo_actor = (actor + 1) % 4;

                // 四風連打
                if self.rule.suufon_renda && self.can_four_wind && self.check_four_wind(pai)? {
                    self.abortive_ryukyoku();
                    return Ok(Poll::End);
                }

                if self.kans == 4 && self.player_states.iter().all(|s| s.kans_count() < 4) {
                    // 四槓散了
                    self.check_four_kan = true;
                }
            }

            Event::Chi { .. } | Event::Pon { .. } => {
                self.check_riichi_accepted();
                self.broadcast(&ev.event);
                self.add_log(ev.clone());
            }

            Event::Ankan { actor, .. } => {
                // For continuous kan
                if self.need_new_dora_at_discard.take().is_some() {
                    self.add_new_dora()?;
                }

                self.broadcast(&ev.event);
                self.add_log(ev.clone());

                // Immediately add new dora
                self.add_new_dora()?;

                self.tsumo_actor = actor;
                self.deal_from_rinshan = Some(());
                self.kans += 1;
            }

            Event::Daiminkan { actor, .. } | Event::Kakan { actor, .. } => {
                // For Kakan only, do not `.take()` it.
                if self.need_new_dora_at_discard.is_some() {
                    self.need_new_dora_at_tsumo = Some(());
                }

                // For Daiminkan only
                self.check_riichi_accepted();

                self.broadcast(&ev.event);
                self.add_log(ev.clone());

                self.need_new_dora_at_discard = Some(());

                self.tsumo_actor = actor;
                self.deal_from_rinshan = Some(());
                self.kans += 1;
            }

            Event::Reach { actor } => {
                self.broadcast(&ev.event);
                self.add_log(ev.clone());
                self.riichi_to_be_accepted = Some(actor);
            }

            Event::Hora { actor, target, .. } => {
                self.handle_hora(actor, target, reactions)?;
                return Ok(Poll::End);
            }

            Event::Ryukyoku { .. } => {
                // 九種九牌
                if !self.rule.kyuushu_kyuuhai {
                    bail!("九種九牌 is disabled by the rule config");
                }
                self.abortive_ryukyoku();
                return Ok(Poll::End);
            }

            _ => {
                bail!("unexpected event: {:?}", ev.event);
            }
        };

        // The pao check cannot be done before the current event (Pon or
        // Daiminkan) gets processed because it requires to read `.pons()` and
        // `.minkans()`.
        self.update_paos(&ev.event);

        Ok(Poll::InGame)
    }

    pub fn encode_oracle_obs(&self, perspective: u8, version: u32) -> Array2<f32> {
        let shape = oracle_obs_shape(version);
        let mut arr = Simple2DArray::<34, f32>::new(shape.0);
        let mut idx = 0;

        self.player_states
            .iter()
            .cycle()
            .skip(perspective as usize + 1)
            .take(3)
            .for_each(|state| {
                state
                    .tehai()
                    .iter()
                    .enumerate()
                    .filter(|&(_, &count)| count > 0)
                    .for_each(|(tile_id, &count)| {
                        arr.assign_rows(idx, tile_id, count as usize, 1.);
                    });
                idx += 4;

                state
                    .akas_in_hand()
                    .iter()
                    .enumerate()
                    .filter(|&(_, &has_it)| has_it)
                    .for_each(|(i, _)| arr.fill(idx + i, 1.));
                idx += 3;

                let n = state.shanten() as usize;
                match version {
                    1 => {
                        arr.fill_rows(idx, n, 1.);
                        idx += 6;
                    }
                    2 | 3 | 4 => {
                        arr.fill(idx + n, 1.);
                        idx += 7;

                        let v = n as f32 / 6.;
                        arr.fill(idx, v);
                        idx += 1;
                    }
                    _ => unreachable!(),
                }

                state
                    .waits()
                    .iter()
                    .enumerate()
                    .filter(|&(_, &c)| c)
                    .for_each(|(t, _)| arr.assign(idx, t, 1.));
                idx += 1;

                if state.at_furiten() {
                    arr.fill(idx, 1.);
                }
                idx += 1;
            });

        let mut encode_tile = |idx: usize, tile: Tile| {
            let tile_id = tile.deaka().as_usize();
            arr.assign(idx, tile_id, 1.);
            if tile.is_aka() {
                arr.fill(idx + 1, 1.);
            }
        };

        self.board
            .yama
            .iter()
            .copied()
            .rev()
            .take(self.tiles_left as usize)
            .for_each(|tile| {
                encode_tile(idx, tile);
                idx += 2;
            });
        idx += (69 - self.tiles_left as usize) * 2;

        self.board.rinshan.iter().copied().rev().for_each(|tile| {
            encode_tile(idx, tile);
            idx += 2;
        });
        idx += (4 - self.board.rinshan.len()) * 2;

        self.dora_indicators_full
            .iter()
            .copied()
            .rev()
            .for_each(|tile| {
                encode_tile(idx, tile);
                idx += 2;
            });

        self.board.ura_indicators.iter().copied().for_each(|tile| {
            encode_tile(idx, tile);
            idx += 2;
        });

        assert_eq!(idx, shape.0);
        arr.build()
    }
}

#[rustfmt::skip]
const UNSHUFFLED: [Tile; 136] = [
    t!(1m),  t!(1m), t!(1m), t!(1m),
    t!(2m),  t!(2m), t!(2m), t!(2m),
    t!(3m),  t!(3m), t!(3m), t!(3m),
    t!(4m),  t!(4m), t!(4m), t!(4m),
    t!(5mr), t!(5m), t!(5m), t!(5m),
    t!(6m),  t!(6m), t!(6m), t!(6m),
    t!(7m),  t!(7m), t!(7m), t!(7m),
    t!(8m),  t!(8m), t!(8m), t!(8m),
    t!(9m),  t!(9m), t!(9m), t!(9m),

    t!(1p),  t!(1p), t!(1p), t!(1p),
    t!(2p),  t!(2p), t!(2p), t!(2p),
    t!(3p),  t!(3p), t!(3p), t!(3p),
    t!(4p),  t!(4p), t!(4p), t!(4p),
    t!(5pr), t!(5p), t!(5p), t!(5p),
    t!(6p),  t!(6p), t!(6p), t!(6p),
    t!(7p),  t!(7p), t!(7p), t!(7p),
    t!(8p),  t!(8p), t!(8p), t!(8p),
    t!(9p),  t!(9p), t!(9p), t!(9p),

    t!(1s),  t!(1s), t!(1s), t!(1s),
    t!(2s),  t!(2s), t!(2s), t!(2s),
    t!(3s),  t!(3s), t!(3s), t!(3s),
    t!(4s),  t!(4s), t!(4s), t!(4s),
    t!(5sr), t!(5s), t!(5s), t!(5s),
    t!(6s),  t!(6s), t!(6s), t!(6s),
    t!(7s),  t!(7s), t!(7s), t!(7s),
    t!(8s),  t!(8s), t!(8s), t!(8s),
    t!(9s),  t!(9s), t!(9s), t!(9s),

    t!(E), t!(E), t!(E), t!(E),
    t!(S), t!(S), t!(S), t!(S),
    t!(W), t!(W), t!(W), t!(W),
    t!(N), t!(N), t!(N), t!(N),
    t!(P), t!(P), t!(P), t!(P),
    t!(F), t!(F), t!(F), t!(F),
    t!(C), t!(C), t!(C), t!(C),
];

#[cfg(test)]
mod test {
    use super::*;

    /// Oya is tenpai (123 456 789m 11p EE); everyone else is clearly noten.
    fn wall_board(rule: RuleConfig) -> BoardState {
        let tenpai = [
            t!(1m), t!(2m), t!(3m), t!(4m), t!(5m), t!(6m), t!(7m), t!(8m), t!(9m), t!(1p),
            t!(1p), t!(E), t!(E),
        ];
        let noten = [
            t!(1m), t!(1m), t!(1m), t!(2m), t!(2m), t!(2m), t!(3m), t!(3m), t!(3m), t!(4m),
            t!(5p), t!(7s), t!(9p),
        ];
        let board = Board {
            kyoku: 0,
            rule,
            haipai: [tenpai, noten, noten, noten],
            // 70 tiles, each kind within the global 4-copy limit w.r.t. the
            // haipai; players tsumogiri everything so the kawa accumulates.
            yama: {
                let mut yama = Vec::new();
                for tile in [
                    t!(1s), t!(2s), t!(3s), t!(4s), t!(6s), t!(8s), t!(9s), t!(2p), t!(3p),
                    t!(4p), t!(6p), t!(7p), t!(8p), t!(P), t!(S), t!(W), t!(N),
                ] {
                    yama.extend([tile; 4]);
                }
                yama.extend([t!(5m), t!(5m)]);
                yama
            },
            dora_indicators: vec![t!(6m)],
            ..Default::default()
        };
        board.into_state()
    }

    /// Drives a full kyoku where everyone tsumogiri's. `declare_noten_oya`
    /// controls whether the oya declares noten when the declaration round
    /// opens. Returns the kyoku result.
    fn run_to_exhaustive(mut board: BoardState, declare_noten_oya: bool) -> KyokuResult {
        let mut reactions: [EventExt; 4] = Default::default();
        let mut rounds = 0;
        loop {
            rounds += 1;
            assert!(rounds < 500, "kyoku did not end");
            match board.poll(reactions).unwrap() {
                Poll::InGame => {
                    reactions = Default::default();
                    if board.noten_phase {
                        if declare_noten_oya {
                            reactions[board.oya as usize] =
                                EventExt::no_meta(Event::Ryukyoku { deltas: None });
                        }
                        continue;
                    }
                    for (actor, state) in board.player_states.iter().enumerate() {
                        if state.last_cans().can_discard {
                            let pai = state.last_self_tsumo().unwrap();
                            reactions[actor] = EventExt::no_meta(Event::Dahai {
                                actor: actor as u8,
                                pai,
                                tsumogiri: true,
                            });
                        }
                    }
                }
                Poll::End => return board.end(),
            }
        }
    }

    #[test]
    fn exhaustive_voluntary_noten_oya() {
        let rule = RuleConfig {
            voluntary_noten: true,
            ..RuleConfig::zhuyi()
        };
        let board = wall_board(rule);

        // Without a declaration the oya is tenpai: renchan and +3000.
        let result = run_to_exhaustive(board, false);
        assert!(result.can_renchan);
        assert_eq!(result.scores, [3000, -1000, -1000, -1000]);

        // Declaring noten gives up the renchan and the noten sticks.
        let board = wall_board(rule);
        let result = run_to_exhaustive(board, true);
        assert!(!result.can_renchan);
        assert_eq!(result.scores, [0; 4]);
    }

    #[test]
    fn exhaustive_no_voluntary_noten_by_default() {
        // Default (Tenhou) rule: no declaration round at all.
        let board = wall_board(RuleConfig::default());
        let result = run_to_exhaustive(board, false);
        assert!(result.can_renchan);
        assert_eq!(result.scores, [3000, -1000, -1000, -1000]);
    }
}
