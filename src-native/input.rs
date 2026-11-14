//! Pure input state machine. Native event replay is marked and posted through the tap proxy.
#[derive(Debug, PartialEq)]
pub enum Effect {
    None,
    Replay,
    Open,
    Select,
    Cancel,
}
#[derive(Default, Debug)]
pub struct Gesture {
    pub held: bool,
    pub open: bool,
    pub cancelled: bool,
    since: f64,
}
impl Gesture {
    pub fn down(&mut self, now: f64) {
        if !self.held {
            self.held = true;
            self.open = false;
            self.cancelled = false;
            self.since = now;
        }
    }
    pub fn tick(&mut self, now: f64) -> Effect {
        if self.held && !self.open && !self.cancelled && now - self.since >= 0.15 {
            self.open = true;
            Effect::Open
        } else {
            Effect::None
        }
    }
    pub fn up(&mut self) -> Effect {
        if !self.held {
            return Effect::None;
        }
        let e = if self.cancelled {
            Effect::None
        } else if self.open {
            Effect::Select
        } else {
            Effect::Replay
        };
        *self = Self::default();
        e
    }
    pub fn interrupt(&mut self) -> Effect {
        if !self.held {
            Effect::None
        } else {
            let e = if self.open {
                Effect::Cancel
            } else if self.cancelled {
                Effect::None
            } else {
                Effect::Replay
            };
            self.cancelled = true;
            self.open = false;
            e
        }
    }
    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.open = false;
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
const KEYS: &[(u16, u16)] = &[
    (65, 0x00),
    (66, 0x0B),
    (67, 0x08),
    (68, 0x02),
    (69, 0x0E),
    (70, 0x03),
    (71, 0x05),
    (72, 0x04),
    (73, 0x22),
    (74, 0x26),
    (75, 0x28),
    (76, 0x25),
    (77, 0x2E),
    (78, 0x2D),
    (79, 0x1F),
    (80, 0x23),
    (81, 0x0C),
    (82, 0x0F),
    (83, 0x01),
    (84, 0x11),
    (85, 0x20),
    (86, 0x09),
    (87, 0x0D),
    (88, 0x07),
    (89, 0x10),
    (90, 0x06),
    (48, 0x1D),
    (49, 0x12),
    (50, 0x13),
    (51, 0x14),
    (52, 0x15),
    (53, 0x17),
    (54, 0x16),
    (55, 0x1A),
    (56, 0x1C),
    (57, 0x19),
    (0x70, 0x7A),
    (0x71, 0x78),
    (0x72, 0x63),
    (0x73, 0x76),
    (0x74, 0x60),
    (0x75, 0x61),
    (0x76, 0x62),
    (0x77, 0x64),
    (0x78, 0x65),
    (0x79, 0x6D),
    (0x7A, 0x67),
    (0x7B, 0x6F),
    (0x7C, 0x69),
    (0x7D, 0x6B),
    (0x7E, 0x71),
    (0x7F, 0x6A),
    (0x80, 0x40),
    (0x81, 0x4F),
    (0x82, 0x50),
    (0x83, 0x5A),
    (0x0D, 0x24),
    (0x0D, 0x4C),
    (0x09, 0x30),
    (0x20, 0x31),
    (0x08, 0x33),
    (0x1B, 0x35),
    (0x2E, 0x75),
    (0x2D, 0x72),
    (0x24, 0x73),
    (0x23, 0x77),
    (0x21, 0x74),
    (0x22, 0x79),
    (0x25, 0x7B),
    (0x26, 0x7E),
    (0x27, 0x7C),
    (0x28, 0x7D),
    (0xC0, 0x32),
    (0xBD, 0x1B),
    (0xBB, 0x18),
    (0xDB, 0x21),
    (0xDD, 0x1E),
    (0xDC, 0x2A),
    (0xBA, 0x29),
    (0xDE, 0x27),
    (0xBC, 0x2B),
    (0xBE, 0x2F),
    (0xBF, 0x2C),
    (0x60, 0x52),
    (0x61, 0x53),
    (0x62, 0x54),
    (0x63, 0x55),
    (0x64, 0x56),
    (0x65, 0x57),
    (0x66, 0x58),
    (0x67, 0x59),
    (0x68, 0x5B),
    (0x69, 0x5C),
    (0x6A, 0x43),
    (0x6B, 0x45),
    (0x6D, 0x4E),
    (0x6E, 0x41),
    (0x6F, 0x4B),
    (0x14, 0x39),
    (0xA0, 0x38),
    (0xA1, 0x3C),
    (0xA2, 0x3B),
    (0xA3, 0x3E),
    (0xA4, 0x3A),
    (0xA5, 0x3D),
    (0x5B, 0x37),
    (0x5C, 0x36),
];
pub fn vk_to_mac(vk: u16) -> Option<u16> {
    KEYS.iter().find(|(v, _)| *v == vk).map(|(_, c)| *c)
}
pub fn mac_to_vk(code: u16) -> Option<u16> {
    KEYS.iter().find(|(_, c)| *c == code).map(|(v, _)| *v)
}
pub fn safe_trigger(vk: u16) -> bool {
    vk_to_mac(vk).is_some()
        && !matches!(vk,0x08|0x09|0x0d|0x10..=0x14|0x1b|0x20|0x25..=0x28|0x5b..=0x5c|0xa0..=0xa5)
}
pub fn key_label(vk: u16) -> String {
    match vk {
        65..=90 | 48..=57 => (vk as u8 as char).to_string(),
        112..=131 => format!("F{}", vk - 111),
        0xc0 => "`".into(),
        0xbd => "−".into(),
        0xbb => "=".into(),
        0xdb => "[".into(),
        0xdd => "]".into(),
        0xdc => "\\".into(),
        0xba => ";".into(),
        0xde => "'".into(),
        0xbc => ",".into(),
        0xbe => ".".into(),
        0xbf => "/".into(),
        _ => format!("键 {vk}"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tap_replays_but_hold_selects() {
        let mut g = Gesture::default();
        g.down(0.);
        assert_eq!(g.up(), Effect::Replay);
        g.down(1.);
        assert_eq!(g.tick(1.16), Effect::Open);
        assert_eq!(g.up(), Effect::Select);
    }
    #[test]
    fn typing_interrupt_replays_once() {
        let mut g = Gesture::default();
        g.down(0.);
        assert_eq!(g.interrupt(), Effect::Replay);
        assert_eq!(g.interrupt(), Effect::None);
        assert_eq!(g.up(), Effect::None);
    }
    #[test]
    fn cancellation_and_reset_do_not_launch() {
        let mut g = Gesture::default();
        g.down(0.);
        g.tick(1.);
        g.cancel();
        assert_eq!(g.up(), Effect::None);
        g.down(2.);
        g.reset();
        assert_eq!(g.up(), Effect::None);
    }
    #[test]
    fn keycodes_are_not_windows_vk() {
        assert_eq!(vk_to_mac(86), Some(9));
        for v in 65..=90 {
            assert_eq!(mac_to_vk(vk_to_mac(v).unwrap()), Some(v));
        }
    }
}
