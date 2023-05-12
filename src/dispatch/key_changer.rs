use std::{
    collections::{BTreeMap, VecDeque},
    rc::Rc,
};

use x11_dl::xlib::NoSymbol;

use crate::xdl::{with_xl, XlibErr, get_x, XlibDpy};

pub(crate) struct SymCode {
    def_syms: BTreeMap<u32, (u8, bool)>,
    mut_syms: VecDeque<(u32, u8)>,
    /// keycode of shift key
    shift: u8,
    xl: Rc<XlibDpy>,
    uu: SpecUni,
}

impl Drop for SymCode {
    fn drop(&mut self) {
        for &(s, k) in self.mut_syms.iter() {
            if s == 0 {
                continue;
            }
            debug!("releasing key {k}, removing keysym {s}");
            if let Err(e) = self.xl.change_key_mapping(k, NoSymbol as _) {
                error!("e {e:?}")
            }
        }
        self.xl.sync();
    }
}

pub(crate) enum SymGroup {
    Old,
    Shift,
    New,
}

impl SymGroup {
    pub fn is_new(&self) -> bool {
        matches!(self, SymGroup::New)
    }
    pub fn is_shift(&self) -> bool {
        matches!(self, SymGroup::Shift)
    }
}

impl SymCode {
    pub(crate) fn new() -> Result<Self, XlibErr> {
        let xl: Rc<XlibDpy> = get_x().unwrap();
        let (cs, shif) = with_xl(|r| {
            let x = r.unwrap();
            (x.codes_syms(), x.modifier_codes().shift())
        });
        let cs = cs?;
        let mut v: VecDeque<_> = cs.unassigned().take(5).map(|c| (0, c)).collect();
        if v.is_empty() {
            warn!("no unassigned keycode");
            v.push_back((0, cs.fallback_unuse()));
        }
        Ok(Self {
            def_syms: cs.sym_key_code(),
            mut_syms: v,
            shift: shif,
            xl,
            uu: SpecUni::new(),
        })
    }
    pub fn shift_key(&self) -> u8 {
        self.shift
    }
    /// actually 1 char
    pub(crate) fn find_sym_str(&mut self, s: &str) -> (u8, SymGroup) {
        let charend = (1..s.len())
            .find(|&i| s.is_char_boundary(i))
            .unwrap_or(s.len());
        let chs = &s[..charend];
        debug_assert!(!chs.is_empty());
        let ch = chs.chars().next().unwrap();
        let chan = ch as u32;
        if chan <= 0x7F || (0xa0..=0xff).contains(&chan) {
            return self.find_sym(chan);
        }
        if chan < u16::MAX as _ && self.uu.contains(chan as u16) {
            debug!("{ch} has legacy keysym");
            // does xlib convert unicode to keysym
        }
        self.find_sym(ch as u32 + 0x1000000)
    }
    pub fn affirm(&self) {
        with_xl(|r| {
            let x = r.unwrap();
            self.mut_syms.iter().filter(|&(s, _c)| *s != 0).for_each(|&(s, c)| {
                if let Err(e) = x.change_key_mapping(c, s as _) {
                    error!("change_key {e:?}");
                };
            });
        });
    }
    pub(crate) fn find_sym(&mut self, s: u32) -> (u8, SymGroup) {
        if let Some(&(c, shift)) = self.def_syms.get(&s) {
            return (
                c,
                if shift {
                    SymGroup::Shift
                } else {
                    SymGroup::Old
                },
            );
        }
        let (sym, c) = *self.mut_syms.front().unwrap();
        if sym == s {
            debug!("{s:x} already mapped to {c}");
            return (c, SymGroup::Old);
        }
        self.mut_syms.rotate_left(1);
        let fr = self.mut_syms.front_mut().unwrap();
        fr.0 = s;
        let c = fr.1;
        with_xl(|r| {
            let x = r.unwrap();
            debug!("mapping {s:x} to {c}");
            if let Err(e) = x.change_key_mapping(c, s as _) {
                error!("change_key {e:?}");
            };
            x.sync();
        });
        (fr.1, SymGroup::New)
    }
}

struct SpecUni {
    not_u_ranges: BTreeMap<u16, u16>,
}

impl SpecUni {
    fn contains(&self, uni: u16) -> bool {
        self.not_u_ranges
            .range(..=uni)
            .rev()
            .next()
            .map(|(_b, &e)| uni <= e)
            .unwrap_or(false)
    }
    fn new() -> Self {
        let not_straight_map = [
            (0x100, 0x113),
            (0x116, 0x12b),
            (0x12e, 0x131),
            (0x134, 0x13e),
            (0x141, 0x148),
            (0x14a, 0x14d),
            (0x150, 0x173),
            (0x178u16, 0x17e),
            (0x192, 0x192),
            (0x2c7, 0x2c7),
            (0x2d8, 0x2d9),
            (0x2db, 0x2db),
            (0x2dd, 0x2dd),
            (0x385, 0x386),
            (0x388, 0x38au16),
            (0x38c, 0x38c),
            (0x38e, 0x3a1),
            (0x3a3, 0x3ce),
            (0x401, 0x40c),
            (0x40e, 0x44f),
            (0x451, 0x45c),
            (0x45e, 0x45f),
            (0x490, 0x491),
            (0x5d0, 0x5ea),
            (0x60c, 0x60c),
            (0x61b, 0x61b),
            (0x61f, 0x61f),
            (0x621, 0x63a),
            (0x640, 0x652),
            (0xe01, 0xe3a),
            (0xe3f, 0xe4d),
            (0xe50, 0xe59),
            (0x11a8, 0x11c2),
            (0x11eb, 0x11eb),
            (0x11f0, 0x11f0),
            (0x11f9, 0x11f9),
            (0x2002, 0x2005),
            (0x2007, 0x200a),
            (0x2012, 0x2015),
            (0x2017, 0x201a),
            (0x201c, 0x201e),
            (0x2020, 0x2022),
            (0x2025, 0x2026),
            (0x2030, 0x2030),
            (0x2032, 0x2033),
            (0x2038, 0x2038),
            (0x203e, 0x203e),
            (0x20a9, 0x20a9),
            (0x20ac, 0x20ac),
            (0x2105, 0x2105),
            (0x2116, 0x2117),
            (0x211e, 0x211e),
            (0x2122, 0x2122),
            (0x2153, 0x215e),
            (0x2190, 0x2193),
            (0x21d2, 0x21d2),
            (0x21d4, 0x21d4),
            (0x2202, 0x2202),
            (0x2207, 0x2207),
            (0x2218, 0x2218),
            (0x221a, 0x221a),
            (0x221d, 0x221e),
            (0x2227, 0x222b),
            (0x2234, 0x2234),
            (0x223c, 0x223c),
            (0x2243, 0x2243),
            (0x2260, 0x2261),
            (0x2264, 0x2265),
            (0x2282, 0x2283),
            (0x22a2, 0x22a5),
            (0x2308, 0x2308),
            (0x230a, 0x230a),
            (0x2315, 0x2315),
            (0x2320, 0x2321),
            (0x2329, 0x232a),
            (0x2395, 0x2395),
            (0x239b, 0x239b),
            (0x239d, 0x239e),
            (0x23a0, 0x23a1),
            (0x23a3, 0x23a4),
            (0x23a6, 0x23a6),
            (0x23a8, 0x23a8),
            (0x23ac, 0x23ac),
            (0x23b7, 0x23b7),
            (0x23ba, 0x23bd),
            (0x2409, 0x240d),
            (0x2423, 0x2424),
            (0x2500, 0x2500),
            (0x2502, 0x2502),
            (0x250c, 0x250c),
            (0x2510, 0x2510),
            (0x2514, 0x2514),
            (0x2518, 0x2518),
            (0x251c, 0x251c),
            (0x2524, 0x2524),
            (0x252c, 0x252c),
            (0x2534, 0x2534),
            (0x253c, 0x253c),
            (0x2592, 0x2592),
            (0x25aa, 0x25af),
            (0x25b2, 0x25b3),
            (0x25b6, 0x25b7),
            (0x25bc, 0x25bd),
            (0x25c0, 0x25c1),
            (0x25c6, 0x25c6),
            (0x25cb, 0x25cb),
            (0x25cf, 0x25cf),
            (0x25e6, 0x25e6),
            (0x2606, 0x2606),
            (0x260e, 0x260e),
            (0x2613, 0x2613),
            (0x261c, 0x261c),
            (0x261e, 0x261e),
            (0x2640, 0x2640),
            (0x2642, 0x2642),
            (0x2663, 0x2663),
            (0x2665, 0x2666),
            (0x266d, 0x266d),
            (0x266f, 0x266f),
            (0x2713, 0x2713),
            (0x2717, 0x2717),
            (0x271d, 0x271d),
            (0x2720, 0x2720),
            (0x3001, 0x3002),
            (0x300c, 0x300d),
            (0x309b, 0x309c),
            (0x30a1, 0x30ab),
            (0x30ad, 0x30ad),
            (0x30af, 0x30af),
            (0x30b1, 0x30b1),
            (0x30b3, 0x30b3),
            (0x30b5, 0x30b5),
            (0x30b7, 0x30b7),
            (0x30b9, 0x30b9),
            (0x30bb, 0x30bb),
            (0x30bd, 0x30bd),
            (0x30bf, 0x30bf),
            (0x30c1, 0x30c1),
            (0x30c3, 0x30c4),
            (0x30c6, 0x30c6),
            (0x30c8, 0x30c8),
            (0x30ca, 0x30cf),
            (0x30d2, 0x30d2),
            (0x30d5, 0x30d5),
            (0x30d8, 0x30d8),
            (0x30db, 0x30db),
            (0x30de, 0x30ed),
            (0x30ef, 0x30ef),
            (0x30f2, 0x30f3),
            (0x30fb, 0x30fc),
            (0x3131, 0x3163),
            (0x316d, 0x316d),
            (0x3171, 0x3171),
            (0x3178, 0x3178),
            (0x317f, 0x317f),
            (0x3181, 0x3181),
            (0x3184, 0x3184),
            (0x3186, 0x3186),
            (0x318D, 0x318E),
        ];
        Self {
            not_u_ranges: not_straight_map.into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn knu() {
        let ks = [
            0x0100, 0x0101, 0x0102, 0x0103, 0x0104, 0x0105, 0x0106, 0x0107, 0x0108, 0x0109, 0x010A,
            0x010B, 0x010C, 0x010D, 0x010E, 0x010F, 0x0110, 0x0111, 0x0112, 0x0113, 0x0116, 0x0117,
            0x0118, 0x0119, 0x011A, 0x011B, 0x011C, 0x011D, 0x011E, 0x011F, 0x0120, 0x0121, 0x0122,
            0x0123, 0x0124, 0x0125, 0x0126, 0x0127, 0x0128, 0x0129, 0x012A, 0x012B, 0x012E, 0x012F,
            0x0130, 0x0131, 0x0134, 0x0135, 0x0136, 0x0137, 0x0138, 0x0139, 0x013A, 0x013B, 0x013C,
            0x013D, 0x013E, 0x0141, 0x0142, 0x0143, 0x0144, 0x0145, 0x0146, 0x0147, 0x0148, 0x014A,
            0x014B, 0x014C, 0x014D, 0x0150, 0x0151, 0x0152, 0x0153, 0x0154, 0x0155, 0x0156, 0x0157,
            0x0158, 0x0159, 0x015A, 0x015B, 0x015C, 0x015D, 0x015E, 0x015F, 0x0160, 0x0161, 0x0162,
            0x0163, 0x0164, 0x0165, 0x0166, 0x0167, 0x0168, 0x0169, 0x016A, 0x016B, 0x016C, 0x016D,
            0x016E, 0x016F, 0x0170, 0x0171, 0x0172, 0x0173, 0x0178, 0x0179, 0x017A, 0x017B, 0x017C,
            0x017D, 0x017E, 0x0192, 0x02C7, 0x02D8, 0x02D9, 0x02DB, 0x02DD, 0x0385, 0x0386, 0x0388,
            0x0389, 0x038A, 0x038C, 0x038E, 0x038F, 0x0390, 0x0391, 0x0392, 0x0393, 0x0394, 0x0395,
            0x0396, 0x0397, 0x0398, 0x0399, 0x039A, 0x039B, 0x039C, 0x039D, 0x039E, 0x039F, 0x03A0,
            0x03A1, 0x03A3, 0x03A4, 0x03A5, 0x03A6, 0x03A7, 0x03A8, 0x03A9, 0x03AA, 0x03AB, 0x03AC,
            0x03AD, 0x03AE, 0x03AF, 0x03B0, 0x03B1, 0x03B2, 0x03B3, 0x03B4, 0x03B5, 0x03B6, 0x03B7,
            0x03B8, 0x03B9, 0x03BA, 0x03BB, 0x03BC, 0x03BD, 0x03BE, 0x03BF, 0x03C0, 0x03C1, 0x03C2,
            0x03C3, 0x03C4, 0x03C5, 0x03C6, 0x03C7, 0x03C8, 0x03C9, 0x03CA, 0x03CB, 0x03CC, 0x03CD,
            0x03CE, 0x0401, 0x0402, 0x0403, 0x0404, 0x0405, 0x0406, 0x0407, 0x0408, 0x0409, 0x040A,
            0x040B, 0x040C, 0x040E, 0x040F, 0x0410, 0x0411, 0x0412, 0x0413, 0x0414, 0x0415, 0x0416,
            0x0417, 0x0418, 0x0419, 0x041A, 0x041B, 0x041C, 0x041D, 0x041E, 0x041F, 0x0420, 0x0421,
            0x0422, 0x0423, 0x0424, 0x0425, 0x0426, 0x0427, 0x0428, 0x0429, 0x042A, 0x042B, 0x042C,
            0x042D, 0x042E, 0x042F, 0x0430, 0x0431, 0x0432, 0x0433, 0x0434, 0x0435, 0x0436, 0x0437,
            0x0438, 0x0439, 0x043A, 0x043B, 0x043C, 0x043D, 0x043E, 0x043F, 0x0440, 0x0441, 0x0442,
            0x0443, 0x0444, 0x0445, 0x0446, 0x0447, 0x0448, 0x0449, 0x044A, 0x044B, 0x044C, 0x044D,
            0x044E, 0x044F, 0x0451, 0x0452, 0x0453, 0x0454, 0x0455, 0x0456, 0x0457, 0x0458, 0x0459,
            0x045A, 0x045B, 0x045C, 0x045E, 0x045F, 0x0490, 0x0491, 0x05D0, 0x05D1, 0x05D2, 0x05D3,
            0x05D4, 0x05D5, 0x05D6, 0x05D7, 0x05D8, 0x05D9, 0x05DA, 0x05DB, 0x05DC, 0x05DD, 0x05DE,
            0x05DF, 0x05E0, 0x05E1, 0x05E2, 0x05E3, 0x05E4, 0x05E5, 0x05E6, 0x05E7, 0x05E8, 0x05E9,
            0x05EA, 0x060C, 0x061B, 0x061F, 0x0621, 0x0622, 0x0623, 0x0624, 0x0625, 0x0626, 0x0627,
            0x0628, 0x0629, 0x062A, 0x062B, 0x062C, 0x062D, 0x062E, 0x062F, 0x0630, 0x0631, 0x0632,
            0x0633, 0x0634, 0x0635, 0x0636, 0x0637, 0x0638, 0x0639, 0x063A, 0x0640, 0x0641, 0x0642,
            0x0643, 0x0644, 0x0645, 0x0646, 0x0647, 0x0648, 0x0649, 0x064A, 0x064B, 0x064C, 0x064D,
            0x064E, 0x064F, 0x0650, 0x0651, 0x0652, 0x0E01, 0x0E02, 0x0E03, 0x0E04, 0x0E05, 0x0E06,
            0x0E07, 0x0E08, 0x0E09, 0x0E0A, 0x0E0B, 0x0E0C, 0x0E0D, 0x0E0E, 0x0E0F, 0x0E10, 0x0E11,
            0x0E12, 0x0E13, 0x0E14, 0x0E15, 0x0E16, 0x0E17, 0x0E18, 0x0E19, 0x0E1A, 0x0E1B, 0x0E1C,
            0x0E1D, 0x0E1E, 0x0E1F, 0x0E20, 0x0E21, 0x0E22, 0x0E23, 0x0E24, 0x0E25, 0x0E26, 0x0E27,
            0x0E28, 0x0E29, 0x0E2A, 0x0E2B, 0x0E2C, 0x0E2D, 0x0E2E, 0x0E2F, 0x0E30, 0x0E31, 0x0E32,
            0x0E33, 0x0E34, 0x0E35, 0x0E36, 0x0E37, 0x0E38, 0x0E39, 0x0E3A, 0x0E3F, 0x0E40, 0x0E41,
            0x0E42, 0x0E43, 0x0E44, 0x0E45, 0x0E46, 0x0E47, 0x0E48, 0x0E49, 0x0E4A, 0x0E4B, 0x0E4C,
            0x0E4D, 0x0E50, 0x0E51, 0x0E52, 0x0E53, 0x0E54, 0x0E55, 0x0E56, 0x0E57, 0x0E58, 0x0E59,
            0x11A8, 0x11A9, 0x11AA, 0x11AB, 0x11AC, 0x11AD, 0x11AE, 0x11AF, 0x11B0, 0x11B1, 0x11B2,
            0x11B3, 0x11B4, 0x11B5, 0x11B6, 0x11B7, 0x11B8, 0x11B9, 0x11BA, 0x11BB, 0x11BC, 0x11BD,
            0x11BE, 0x11BF, 0x11C0, 0x11C1, 0x11C2, 0x11EB, 0x11F0, 0x11F9, 0x2002, 0x2003, 0x2004,
            0x2005, 0x2007, 0x2008, 0x2009, 0x200A, 0x2012, 0x2013, 0x2014, 0x2015, 0x2017, 0x2018,
            0x2019, 0x201A, 0x201C, 0x201D, 0x201E, 0x2020, 0x2021, 0x2022, 0x2025, 0x2026, 0x2030,
            0x2032, 0x2033, 0x2038, 0x203E, 0x20A9, 0x20AC, 0x2105, 0x2116, 0x2117, 0x211E, 0x2122,
            0x2153, 0x2154, 0x2155, 0x2156, 0x2157, 0x2158, 0x2159, 0x215A, 0x215B, 0x215C, 0x215D,
            0x215E, 0x2190, 0x2191, 0x2192, 0x2193, 0x21D2, 0x21D4, 0x2202, 0x2207, 0x2218, 0x221A,
            0x221D, 0x221E, 0x2227, 0x2228, 0x2229, 0x222A, 0x222B, 0x2234, 0x223C, 0x2243, 0x2260,
            0x2261, 0x2264, 0x2265, 0x2282, 0x2283, 0x22A2, 0x22A3, 0x22A4, 0x22A5, 0x2308, 0x230A,
            0x2315, 0x2320, 0x2321, 0x2329, 0x232A, 0x2395, 0x239B, 0x239D, 0x239E, 0x23A0, 0x23A1,
            0x23A3, 0x23A4, 0x23A6, 0x23A8, 0x23AC, 0x23B7, 0x23BA, 0x23BB, 0x23BC, 0x23BD, 0x2409,
            0x240A, 0x240B, 0x240C, 0x240D, 0x2423, 0x2424, 0x2500, 0x2502, 0x250C, 0x2510, 0x2514,
            0x2518, 0x251C, 0x2524, 0x252C, 0x2534, 0x253C, 0x2592, 0x25AA, 0x25AB, 0x25AC, 0x25AD,
            0x25AE, 0x25AF, 0x25B2, 0x25B3, 0x25B6, 0x25B7, 0x25BC, 0x25BD, 0x25C0, 0x25C1, 0x25C6,
            0x25CB, 0x25CF, 0x25E6, 0x2606, 0x260E, 0x2613, 0x261C, 0x261E, 0x2640, 0x2642, 0x2663,
            0x2665, 0x2666, 0x266D, 0x266F, 0x2713, 0x2717, 0x271D, 0x2720, 0x3001, 0x3002, 0x300C,
            0x300D, 0x309B, 0x309C, 0x30A1, 0x30A2, 0x30A3, 0x30A4, 0x30A5, 0x30A6, 0x30A7, 0x30A8,
            0x30A9, 0x30AA, 0x30AB, 0x30AD, 0x30AF, 0x30B1, 0x30B3, 0x30B5, 0x30B7, 0x30B9, 0x30BB,
            0x30BD, 0x30BF, 0x30C1, 0x30C3, 0x30C4, 0x30C6, 0x30C8, 0x30CA, 0x30CB, 0x30CC, 0x30CD,
            0x30CE, 0x30CF, 0x30D2, 0x30D5, 0x30D8, 0x30DB, 0x30DE, 0x30DF, 0x30E0, 0x30E1, 0x30E2,
            0x30E3, 0x30E4, 0x30E5, 0x30E6, 0x30E7, 0x30E8, 0x30E9, 0x30EA, 0x30EB, 0x30EC, 0x30ED,
            0x30EF, 0x30F2, 0x30F3, 0x30FB, 0x30FC, 0x3131, 0x3132, 0x3133, 0x3134, 0x3135, 0x3136,
            0x3137, 0x3138, 0x3139, 0x313A, 0x313B, 0x313C, 0x313D, 0x313E, 0x313F, 0x3140, 0x3141,
            0x3142, 0x3143, 0x3144, 0x3145, 0x3146, 0x3147, 0x3148, 0x3149, 0x314A, 0x314B, 0x314C,
            0x314D, 0x314E, 0x314F, 0x3150, 0x3151, 0x3152, 0x3153, 0x3154, 0x3155, 0x3156, 0x3157,
            0x3158, 0x3159, 0x315A, 0x315B, 0x315C, 0x315D, 0x315E, 0x315F, 0x3160, 0x3161, 0x3162,
            0x3163, 0x316D, 0x3171, 0x3178, 0x317F, 0x3181, 0x3184, 0x3186, 0x318D, 0x318E,
        ];
        let su = SpecUni::new();
        let t: u16 = su.not_u_ranges.iter().map(|(b, e)| e - b + 1).sum();
        let mut lar = 0;
        for (&b, &e) in su.not_u_ranges.iter() {
            assert!(b > lar, "b {b:x}");
            lar = e;
        }
        for k in ks {
            assert!(su.contains(k));
        }
        assert_eq!(t as usize, ks.len());
    }
}