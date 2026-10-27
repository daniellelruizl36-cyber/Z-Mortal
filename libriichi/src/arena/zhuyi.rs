//! Zhuyi (祝仪) settlement: per-hora cash counters, counted in 枚 (sheets).
//!
//! The counters follow the table convention confirmed with the user on
//! 2026-09-15, identical to `zhuyi/settlement.py` (the cross-check oracle):
//!
//! - base = aka sheets + ura sheets + ippatsu + rinshan + haitei, where aka
//!   and ura are counted over the whole winning hand including melds, and
//!   3+ aka (ALL STAR, incl. melds) upgrades the aka part to 5;
//! - yakuman: ron = discarder pays 10 per unit, tsumo = each opponent pays 5
//!   per unit; compound yakuman stacks linearly (units = number of yakuman
//!   patterns);
//! - pao (daisangen / daisuushii / suukantsu): each pao entry covers the
//!   unit(s) of the pattern it supplied, settled separately — the supplier
//!   pays only their covered share (tsumo = 10 per covered unit plus the
//!   normal share of the uncovered part; ron = 5 per covered unit), the
//!   discarder / the other opponents pay the rest in full;
//!
//! The result is a delta in 枚 per player; the CU conversion (1 枚 = 20 CU)
//! happens on the Python side.

use serde::{Deserialize, Serialize};

/// The per-hora inputs of a winning hand, counted from `PlayerState`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZhuyiCounts {
    /// Aka dora sheets, incl. melds and the ron-winning tile, ALL STAR
    /// (3+) already upgraded to 5.
    pub aka: u8,
    /// Ura dora sheets over the whole hand incl. melds.
    pub ura: u8,
    pub ippatsu: bool,
    pub rinshan: bool,
    pub haitei: bool,
}

impl ZhuyiCounts {
    /// Total non-yakuman sheets.
    #[inline]
    #[must_use]
    pub const fn base(self) -> u8 {
        self.aka + self.ura + self.ippatsu as u8 + self.rinshan as u8 + self.haitei as u8
    }
}

/// The recorded outcome of one hora, embedded into the mjson metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZhuyiOutcome {
    pub counts: ZhuyiCounts,
    pub yakuman_units: u8,
    /// Net sheets per player (income positive). Zero-sum.
    pub deltas: [i32; 4],
}

/// Compute the zhuyi deltas (in sheets) for one hora.
///
/// `target` is the discarder on ron, `actor` on tsumo. `paos` lists the
/// `(supplier, covered units)` shares: each entry is one pao supplier
/// together with how many of the yakuman units they are responsible for.
/// Covered units are clamped to `yakuman_units` in total.
#[must_use]
pub fn settle(
    actor: u8,
    target: u8,
    counts: ZhuyiCounts,
    yakuman_units: u8,
    paos: &[(u8, u8)],
) -> [i32; 4] {
    let base = counts.base();
    let units = yakuman_units as i32;
    let mut deltas = [0_i32; 4];
    let pay = |payer: u8, sheets: i32, deltas: &mut [i32; 4]| {
        deltas[payer as usize] -= sheets;
        deltas[actor as usize] += sheets;
    };

    // Remaining covered units per supplier, clamped so the shares never
    // exceed the total yakuman units of the hand.
    let mut shares: Vec<(u8, i32)> = Vec::with_capacity(paos.len());
    let mut covered = 0_i32;
    for &(sup, k) in paos {
        let k = (k as i32).clamp(0, units - covered);
        if k > 0 {
            covered += k;
            shares.push((sup, k));
        }
    }
    let uncovered = units - covered;

    if actor == target {
        // tsumo: base from everyone; each supplier pays 10 per covered unit
        // plus the normal 5-per-unit share of the uncovered part, the other
        // opponents pay only the uncovered share.
        for p in 0..4_u8 {
            if p != actor {
                pay(p, base as i32, &mut deltas);
            }
        }
        if units > 0 {
            for p in 0..4_u8 {
                if p == actor || shares.iter().any(|&(s, _)| s == p) {
                    continue;
                }
                pay(p, 5 * uncovered, &mut deltas);
            }
            for &(sup, k) in &shares {
                pay(sup, 10 * k + 5 * uncovered, &mut deltas);
            }
        }
    } else {
        // ron: base from the discarder; discarder pays 10 per uncovered unit
        // plus 5 per covered unit, each supplier pays 5 per covered unit.
        pay(target, base as i32, &mut deltas);
        if units > 0 {
            pay(target, 10 * uncovered + 5 * covered, &mut deltas);
            for &(sup, k) in &shares {
                pay(sup, 5 * k, &mut deltas);
            }
        }
    }

    deltas
}

#[cfg(test)]
mod test {
    use super::*;

    const COUNTS0: ZhuyiCounts = ZhuyiCounts {
        aka: 0,
        ura: 0,
        ippatsu: false,
        rinshan: false,
        haitei: false,
    };

    #[test]
    fn tsumo_base_only() {
        let counts = ZhuyiCounts {
            aka: 2,
            ura: 1,
            ippatsu: true,
            rinshan: false,
            haitei: false,
        };
        // base 4 from each of the 3 opponents
        assert_eq!(settle(0, 0, counts, 0, &[]), [12, -4, -4, -4]);
    }

    #[test]
    fn ron_base_only() {
        let counts = ZhuyiCounts {
            aka: 1,
            ura: 0,
            ippatsu: false,
            rinshan: false,
            haitei: false,
        };
        // base 1 paid by the discarder only
        assert_eq!(settle(2, 1, counts, 0, &[]), [0, -1, 1, 0]);
    }

    #[test]
    fn all_star_upgrades_aka_part() {
        // 3 aka -> 5 sheets, the other parts stack on top
        let counts = ZhuyiCounts {
            aka: 5,
            ura: 2,
            ippatsu: false,
            rinshan: true,
            haitei: false,
        };
        assert_eq!(counts.base(), 8);
        assert_eq!(settle(1, 1, counts, 0, &[]), [-8, 24, -8, -8]);
    }

    #[test]
    fn yakuman_tsumo_splits_three_ways() {
        // 2 units: each opponent pays 2*5 = 10
        assert_eq!(settle(3, 3, COUNTS0, 2, &[]), [-10, -10, -10, 30]);
    }

    #[test]
    fn yakuman_ron_full_from_discarder() {
        // 1 unit: discarder pays 10
        assert_eq!(settle(0, 2, COUNTS0, 1, &[]), [10, 0, -10, 0]);
    }

    #[test]
    fn pao_tsumo_supplier_pays_all() {
        // supplier covers 10*units, others only pay base
        let counts = ZhuyiCounts {
            aka: 1,
            ..COUNTS0
        };
        // base 1 from everyone, plus 20 from pao (seat 2)
        assert_eq!(settle(0, 0, counts, 2, &[(2, 2)]), [23, -1, -21, -1]);
    }

    #[test]
    fn pao_ron_splits_half() {
        // discarder (1) and pao supplier (3) each pay 5*units
        assert_eq!(settle(0, 1, COUNTS0, 1, &[(3, 1)]), [10, -5, 0, -5]);
    }

    #[test]
    fn compound_ron_two_suppliers_each_cover_one() {
        // 2 units, each supplier covers 1: discarder pays 5+5 covered, each
        // supplier pays 5
        assert_eq!(settle(0, 1, COUNTS0, 2, &[(2, 1), (3, 1)]), [20, -10, -5, -5]);
    }

    #[test]
    fn compound_ron_partial_cover() {
        // 3 units, supplier (3) covers 1: discarder pays 10*2 uncovered + 5
        // covered = 25, supplier pays 5
        assert_eq!(settle(0, 1, COUNTS0, 3, &[(3, 1)]), [30, -25, 0, -5]);
    }

    #[test]
    fn compound_ron_single_supplier_covers_two() {
        // 3 units, supplier (2) covers 2: discarder pays 10 + 10 = 20,
        // supplier pays 10
        assert_eq!(settle(1, 0, COUNTS0, 3, &[(2, 2)]), [-20, 30, -10, 0]);
    }

    #[test]
    fn compound_tsumo_partial_cover() {
        // 2 units, supplier (2) covers 1: supplier pays 10 + 5 uncovered =
        // 15, others pay 5 each
        assert_eq!(settle(0, 0, COUNTS0, 2, &[(2, 1)]), [25, -5, -15, -5]);
    }

    #[test]
    fn compound_tsumo_full_cover_two_suppliers() {
        // 2 units fully covered: each supplier pays 10, others pay 0
        assert_eq!(settle(3, 3, COUNTS0, 2, &[(0, 1), (1, 1)]), [-10, -10, 0, 20]);
    }

    #[test]
    fn shares_clamped_to_units() {
        // declared shares exceed the actual units: first share covers the
        // only unit, the rest is clamped away and pays nothing
        assert_eq!(settle(0, 0, COUNTS0, 1, &[(1, 1), (2, 1)]), [10, -10, 0, 0]);
    }

    #[test]
    fn zero_sum() {
        let cases = [
            settle(0, 0, COUNTS0, 0, &[]),
            settle(2, 1, COUNTS0, 3, &[]),
            settle(1, 1, COUNTS0, 1, &[(0, 1)]),
            settle(3, 0, COUNTS0, 2, &[(2, 2)]),
            settle(0, 1, COUNTS0, 4, &[(2, 1), (3, 2)]),
            settle(2, 2, COUNTS0, 3, &[(0, 1), (1, 1), (3, 1)]),
        ];
        for d in cases {
            assert_eq!(d.iter().sum::<i32>(), 0);
        }
    }
}
