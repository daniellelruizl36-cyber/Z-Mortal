//! Custom rule configuration for the zhuyi (祝仪) table.
//!
//! libriichi is hardcoded to Tenhou's rule; this struct introduces opt-in
//! switches for the custom cash-game rule set. The default is exactly the
//! original Tenhou behavior, so existing callers are unaffected.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RuleConfig {
    /// 九種九牌途中流局 (default: on, Tenhou).
    pub kyuushu_kyuuhai: bool,
    /// 四家立直途中流局 (default: on).
    pub suucha_riichi: bool,
    /// 四槓散了途中流局 (default: on).
    pub suukantsu_draw: bool,
    /// 四風連打途中流局 (default: on).
    pub suufon_renda: bool,
    /// 流し満貫 (default: on).
    pub nagashi_mangan: bool,
    /// 13+ han counts as kazoe yakuman (default: on). Off = cap at 3x mangan.
    pub kazoe_yakuman: bool,
    /// Tenhou/chiihou stack with pattern yakuman, e.g. chiihou + kokushi = 2
    /// units (default: off, Tenhou treats them as isolated 1x yakuman).
    pub luck_yakuman_stack: bool,
    /// 切上満貫: 30 fu 4 han and 60 fu 3 han pay mangan (default: off).
    pub kiriage_mangan: bool,
    /// Sudden death at 30000 incl. 西入 and renchan-owari at all-last
    /// (default: on). Off = the game only ends when the oya's renchan stops
    /// (kyoku advances past all-last) or someone goes below zero.
    pub sudden_death: bool,
    /// At an exhaustive draw every player may voluntarily declare noten,
    /// overriding the automatic tenpai detection (default: off). Declaring
    /// noten as oya gives up the renchan.
    pub voluntary_noten: bool,
    /// Pao for four kans: the supplier of the 4th kan (daiminkan) pays for
    /// suukantsu (default: off, Tenhou has no such pao).
    pub suukantsu_pao: bool,
    /// Compute zhuyi (祝仪) counters per hora and record them in the mjson
    /// metadata (default: off, Tenhou has no zhuyi).
    pub zhuyi: bool,
    /// Guangzhou table convention: the aka (red dora) sheets of a hora can
    /// only be collected on a menzen (concealed) win — any open call
    /// (chi/pon/daiminkan; ankan keeps menzen) voids the aka sheets for both
    /// collection and payment (default: off, Japanese tables settle aka
    /// sheets regardless of menzen).
    pub zhuyi_menzen_only: bool,
}

impl Default for RuleConfig {
    fn default() -> Self {
        Self {
            kyuushu_kyuuhai: true,
            suucha_riichi: true,
            suukantsu_draw: true,
            suufon_renda: true,
            nagashi_mangan: true,
            kazoe_yakuman: true,
            luck_yakuman_stack: false,
            kiriage_mangan: false,
            sudden_death: true,
            voluntary_noten: false,
            suukantsu_pao: false,
            zhuyi: false,
            zhuyi_menzen_only: false,
        }
    }
}

impl RuleConfig {
    /// The zhuyi table preset: no abortive draws, no nagashi mangan, no kazoe
    /// yakuman, no multi yakuman enhancements, kiriage mangan for everyone,
    /// no sudden death with forced renchan at all-last, voluntary noten
    /// declaration, and pao for suukantsu.
    #[must_use]
    pub const fn zhuyi() -> Self {
        Self {
            kyuushu_kyuuhai: false,
            suucha_riichi: false,
            suukantsu_draw: false,
            suufon_renda: false,
            nagashi_mangan: false,
            kazoe_yakuman: false,
            luck_yakuman_stack: true,
            kiriage_mangan: true,
            sudden_death: false,
            voluntary_noten: true,
            suukantsu_pao: true,
            zhuyi: true,
            zhuyi_menzen_only: false,
        }
    }

    /// The Guangzhou zhuyi table: the zhuyi preset plus the menzen-only
    /// aka collection rule.
    #[must_use]
    pub const fn zhuyi_guangzhou() -> Self {
        Self {
            zhuyi_menzen_only: true,
            ..Self::zhuyi()
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn default_is_tenhou() {
        let d = RuleConfig::default();
        assert!(d.kyuushu_kyuuhai && d.suucha_riichi && d.suukantsu_draw);
        assert!(d.suufon_renda && d.nagashi_mangan && d.kazoe_yakuman);
        assert!(d.sudden_death);
        assert!(!d.luck_yakuman_stack && !d.kiriage_mangan);
        assert!(!d.voluntary_noten && !d.suukantsu_pao && !d.zhuyi);
    }

    #[test]
    fn zhuyi_preset() {
        let z = RuleConfig::zhuyi();
        assert!(!z.kyuushu_kyuuhai && !z.suucha_riichi && !z.suukantsu_draw);
        assert!(!z.suufon_renda && !z.nagashi_mangan && !z.kazoe_yakuman);
        assert!(!z.sudden_death);
        assert!(z.luck_yakuman_stack && z.kiriage_mangan);
        assert!(z.voluntary_noten && z.suukantsu_pao && z.zhuyi);
    }

    #[test]
    fn json_roundtrip() {
        let z = RuleConfig::zhuyi();
        let s = serde_json::to_string(&z).unwrap();
        let parsed: RuleConfig = serde_json::from_str(&s).unwrap();
        assert_eq!(z, parsed);

        // missing fields fall back to Tenhou default
        let parsed: RuleConfig = serde_json::from_str(r#"{"kiriage_mangan":true}"#).unwrap();
        assert!(parsed.kiriage_mangan);
        assert!(parsed.kyuushu_kyuuhai);
    }
}
